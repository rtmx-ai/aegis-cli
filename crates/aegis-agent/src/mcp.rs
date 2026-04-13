//! MCP (Model Context Protocol) server integration.
//!
//! This module provides the client-side implementation for connecting to
//! MCP servers over stdio (subprocess with JSON-RPC on stdin/stdout) or
//! HTTP+SSE transports. It handles:
//!
//! - Server connection and lifecycle (REQ-AGENT-014)
//! - Tool discovery via `tools/list` (REQ-AGENT-022)
//! - Tool schema marshaling for the LLM (REQ-AGENT-023)
//! - HITL enforcement for MCP tools (REQ-AGENT-024)
//! - Output truncation (REQ-AGENT-025)

use crate::mcp_types::{JsonRpcRequest, JsonRpcResponse};
use crate::truncation::truncate_output;
use aegis_domain::error::DomainError;
use aegis_domain::ports::ToolSchema;
use aegis_domain::types::ToolRisk;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tracing::{debug, info};

/// MCP protocol version supported by this client.
const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// Configuration for an MCP server connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Human-readable name for this server.
    pub name: String,
    /// Transport configuration.
    pub transport: McpTransport,
}

/// Transport type for connecting to an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum McpTransport {
    /// Spawn a subprocess, communicate via stdin/stdout JSON-RPC (NDJSON).
    Stdio {
        command: String,
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
    },
    /// Connect via HTTP+SSE (not yet implemented).
    Sse {
        url: String,
        #[serde(default)]
        headers: HashMap<String, String>,
    },
}

/// A discovered tool from an MCP server.
#[derive(Debug, Clone)]
pub struct McpTool {
    /// Name of the server this tool belongs to.
    pub server_name: String,
    /// Tool name as reported by the server.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// JSON Schema for the tool's input parameters.
    pub input_schema: serde_json::Value,
}

impl McpTool {
    /// MCP tools always return StateMutating risk since their side effects
    /// are unknown to aegis. This ensures all MCP tool calls go through the
    /// HITL approval gate (REQ-AGENT-024).
    pub fn risk(&self) -> ToolRisk {
        ToolRisk::StateMutating
    }

    /// The qualified tool name used when passing schemas to the LLM.
    /// Format: `{server_name}__{tool_name}` to avoid collisions between
    /// servers (REQ-AGENT-023).
    pub fn qualified_name(&self) -> String {
        format!("{}__{}", self.server_name, self.name)
    }

    /// Convert this MCP tool to a ToolSchema for the LLM (REQ-AGENT-023).
    pub fn to_tool_schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.qualified_name(),
            description: self.description.clone(),
            parameters: self.input_schema.clone(),
        }
    }
}

/// An active MCP server connection (stdio transport).
pub struct McpConnection {
    server_name: String,
    transport: ActiveTransport,
    tools: Vec<McpTool>,
}

/// Internal transport state.
enum ActiveTransport {
    Stdio {
        child: tokio::process::Child,
        stdin: BufWriter<tokio::process::ChildStdin>,
        stdout: BufReader<tokio::process::ChildStdout>,
        next_id: u64,
    },
}

impl McpConnection {
    /// Send a JSON-RPC request and receive a response (stdio transport).
    async fn send_request(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<JsonRpcResponse, DomainError> {
        let ActiveTransport::Stdio {
            ref mut stdin,
            ref mut stdout,
            ref mut next_id,
            ..
        } = self.transport;

        let id = *next_id;
        *next_id += 1;

        let request = if let Some(p) = params {
            JsonRpcRequest::with_params(id, method, p)
        } else {
            JsonRpcRequest::new(id, method)
        };

        let serialized = serde_json::to_string(&request).map_err(|e| {
            DomainError::Other(format!("Failed to serialize JSON-RPC request: {e}"))
        })?;

        debug!(
            server = %self.server_name,
            method,
            id,
            "sending MCP request"
        );

        // Write request as a single line (NDJSON)
        stdin.write_all(serialized.as_bytes()).await.map_err(|e| {
            DomainError::Other(format!(
                "Failed to write to MCP server '{}': {e}",
                self.server_name
            ))
        })?;
        stdin.write_all(b"\n").await.map_err(|e| {
            DomainError::Other(format!(
                "Failed to write newline to MCP server '{}': {e}",
                self.server_name
            ))
        })?;
        stdin.flush().await.map_err(|e| {
            DomainError::Other(format!(
                "Failed to flush MCP server '{}': {e}",
                self.server_name
            ))
        })?;

        // Read response line
        let mut line = String::new();
        stdout.read_line(&mut line).await.map_err(|e| {
            DomainError::Other(format!(
                "Failed to read from MCP server '{}': {e}",
                self.server_name
            ))
        })?;

        if line.is_empty() {
            return Err(DomainError::Other(format!(
                "MCP server '{}' closed connection unexpectedly",
                self.server_name
            )));
        }

        let response: JsonRpcResponse = serde_json::from_str(line.trim()).map_err(|e| {
            DomainError::Other(format!(
                "Failed to parse JSON-RPC response from '{}': {e}",
                self.server_name
            ))
        })?;

        Ok(response)
    }

    /// Send a JSON-RPC notification (no response expected).
    async fn send_notification(&mut self, method: &str) -> Result<(), DomainError> {
        let ActiveTransport::Stdio { ref mut stdin, .. } = self.transport;

        let notif = JsonRpcRequest::notification(method);
        let serialized = serde_json::to_string(&notif)
            .map_err(|e| DomainError::Other(format!("Failed to serialize notification: {e}")))?;

        stdin.write_all(serialized.as_bytes()).await.map_err(|e| {
            DomainError::Other(format!(
                "Failed to write notification to '{}': {e}",
                self.server_name
            ))
        })?;
        stdin.write_all(b"\n").await.map_err(|e| {
            DomainError::Other(format!(
                "Failed to write newline to '{}': {e}",
                self.server_name
            ))
        })?;
        stdin.flush().await.map_err(|e| {
            DomainError::Other(format!(
                "Failed to flush notification to '{}': {e}",
                self.server_name
            ))
        })?;

        Ok(())
    }

    /// Perform the MCP initialize handshake.
    async fn initialize(&mut self) -> Result<(), DomainError> {
        let init_params = serde_json::json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {
                "name": "aegis",
                "version": "0.1.0"
            }
        });

        let response = self.send_request("initialize", Some(init_params)).await?;

        if let Some(err) = response.error {
            return Err(DomainError::Other(format!(
                "MCP initialize failed for '{}': {} (code {})",
                self.server_name, err.message, err.code
            )));
        }

        info!(
            server = %self.server_name,
            "MCP initialize handshake complete"
        );

        // Send initialized notification
        self.send_notification("notifications/initialized").await?;

        Ok(())
    }

    /// Discover tools from the MCP server via `tools/list`.
    async fn discover_tools(&mut self) -> Result<Vec<McpTool>, DomainError> {
        let response = self.send_request("tools/list", None).await?;

        if let Some(err) = response.error {
            return Err(DomainError::Other(format!(
                "MCP tools/list failed for '{}': {} (code {})",
                self.server_name, err.message, err.code
            )));
        }

        let result = response.result.ok_or_else(|| {
            DomainError::Other(format!(
                "MCP tools/list returned no result for '{}'",
                self.server_name
            ))
        })?;

        // Parse the tools array from the result
        let tools_value = result.get("tools").ok_or_else(|| {
            DomainError::Other(format!(
                "MCP tools/list response missing 'tools' field for '{}'",
                self.server_name
            ))
        })?;

        let raw_tools: Vec<RawMcpTool> =
            serde_json::from_value(tools_value.clone()).map_err(|e| {
                DomainError::Other(format!(
                    "Failed to parse tools from '{}': {e}",
                    self.server_name
                ))
            })?;

        let tools: Vec<McpTool> = raw_tools
            .into_iter()
            .map(|t| McpTool {
                server_name: self.server_name.clone(),
                name: t.name,
                description: t.description.unwrap_or_default(),
                input_schema: t
                    .input_schema
                    .unwrap_or(serde_json::json!({"type": "object"})),
            })
            .collect();

        info!(
            server = %self.server_name,
            tool_count = tools.len(),
            "discovered MCP tools"
        );

        self.tools = tools.clone();
        Ok(tools)
    }

    /// Execute a tool call on this MCP server.
    async fn call_tool(
        &mut self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<String, DomainError> {
        let params = serde_json::json!({
            "name": tool_name,
            "arguments": arguments,
        });

        let response = self.send_request("tools/call", Some(params)).await?;

        if let Some(err) = response.error {
            return Err(DomainError::Other(format!(
                "MCP tools/call '{}' failed on '{}': {} (code {})",
                tool_name, self.server_name, err.message, err.code
            )));
        }

        let result = response.result.ok_or_else(|| {
            DomainError::Other(format!(
                "MCP tools/call '{}' returned no result from '{}'",
                tool_name, self.server_name
            ))
        })?;

        // MCP tool results contain a `content` array with text/image blocks.
        // Extract text content.
        if let Some(content) = result.get("content")
            && let Some(arr) = content.as_array()
        {
            let text_parts: Vec<&str> = arr
                .iter()
                .filter_map(|item| {
                    if item.get("type")?.as_str()? == "text" {
                        item.get("text")?.as_str()
                    } else {
                        None
                    }
                })
                .collect();
            return Ok(text_parts.join("\n"));
        }

        // Fallback: serialize the entire result as a string
        Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
    }

    /// Shut down the connection, killing the child process if stdio.
    async fn shutdown(&mut self) {
        let ActiveTransport::Stdio { ref mut child, .. } = self.transport;
        let _ = child.kill().await;
        info!(server = %self.server_name, "MCP server process terminated");
    }
}

/// Raw tool definition as returned by MCP `tools/list`.
#[derive(Debug, Deserialize)]
struct RawMcpTool {
    name: String,
    description: Option<String>,
    #[serde(rename = "inputSchema")]
    input_schema: Option<serde_json::Value>,
}

/// The MCP manager holds all active connections and provides a unified
/// interface for tool discovery and execution.
pub struct McpManager {
    connections: Vec<McpConnection>,
}

impl McpManager {
    /// Create a new MCP manager with no connections.
    pub fn new() -> Self {
        Self {
            connections: Vec::new(),
        }
    }

    /// Return the number of active connections.
    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }

    /// Connect to an MCP server and discover its tools (REQ-AGENT-014,
    /// REQ-AGENT-022).
    ///
    /// Spawns the server process (for stdio transport), performs the
    /// JSON-RPC initialize handshake, then calls `tools/list` to discover
    /// available tools.
    pub async fn connect(
        &mut self,
        config: McpServerConfig,
    ) -> Result<Vec<McpTool>, DomainError> {
        match config.transport {
            McpTransport::Stdio { command, args, env } => {
                self.connect_stdio(&config.name, &command, &args, &env)
                    .await
            }
            McpTransport::Sse { .. } => Err(DomainError::Other(
                "SSE transport is not yet implemented".to_string(),
            )),
        }
    }

    /// Connect via stdio transport.
    async fn connect_stdio(
        &mut self,
        name: &str,
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> Result<Vec<McpTool>, DomainError> {
        info!(server = %name, command, "spawning MCP server process");

        let mut cmd = tokio::process::Command::new(command);
        cmd.args(args)
            .envs(env.iter())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            DomainError::Other(format!(
                "Failed to spawn MCP server '{name}' ({command}): {e}"
            ))
        })?;

        let stdin = child.stdin.take().ok_or_else(|| {
            DomainError::Other(format!("Failed to capture stdin for MCP server '{name}'"))
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            DomainError::Other(format!("Failed to capture stdout for MCP server '{name}'"))
        })?;

        let mut conn = McpConnection {
            server_name: name.to_string(),
            transport: ActiveTransport::Stdio {
                child,
                stdin: BufWriter::new(stdin),
                stdout: BufReader::new(stdout),
                next_id: 1,
            },
            tools: Vec::new(),
        };

        // Perform MCP handshake
        conn.initialize().await?;

        // Discover tools
        let tools = conn.discover_tools().await?;

        self.connections.push(conn);
        Ok(tools)
    }

    /// Get all discovered tools as ToolSchemas for the LLM (REQ-AGENT-023).
    ///
    /// Tool names are prefixed with the server name to avoid collisions:
    /// `{server_name}__{tool_name}`.
    pub fn tool_schemas(&self) -> Vec<ToolSchema> {
        self.connections
            .iter()
            .flat_map(|conn| conn.tools.iter().map(|tool| tool.to_tool_schema()))
            .collect()
    }

    /// Get all discovered MCP tools.
    pub fn all_tools(&self) -> Vec<&McpTool> {
        self.connections
            .iter()
            .flat_map(|conn| conn.tools.iter())
            .collect()
    }

    /// Execute a tool call on the appropriate MCP server (REQ-AGENT-025).
    ///
    /// The `qualified_name` should be in `{server_name}__{tool_name}` format.
    /// Output is truncated using the same logic as built-in tools.
    pub async fn execute(
        &mut self,
        qualified_name: &str,
        arguments: serde_json::Value,
    ) -> Result<String, DomainError> {
        // Parse server_name and tool_name from qualified name
        let (server_name, tool_name) = qualified_name.split_once("__").ok_or_else(|| {
            DomainError::Other(format!(
                "Invalid MCP tool name format (expected \
                     'server__tool'): {qualified_name}"
            ))
        })?;

        let conn = self
            .connections
            .iter_mut()
            .find(|c| c.server_name == server_name)
            .ok_or_else(|| {
                DomainError::Other(format!(
                    "No MCP server connection found for '{server_name}'"
                ))
            })?;

        let raw_output = conn.call_tool(tool_name, arguments).await?;

        // REQ-AGENT-025: Truncate large outputs using the same logic
        // as built-in tools.
        Ok(truncate_output(&raw_output))
    }

    /// Disconnect all MCP servers and kill child processes.
    pub async fn shutdown(&mut self) {
        for conn in &mut self.connections {
            conn.shutdown().await;
        }
        self.connections.clear();
        info!("all MCP server connections shut down");
    }
}

impl Default for McpManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse a qualified MCP tool name into (server_name, tool_name).
pub fn parse_qualified_name(qualified: &str) -> Option<(&str, &str)> {
    qualified.split_once("__")
}

/// Check if a tool name looks like an MCP tool (contains `__` separator).
pub fn is_mcp_tool(name: &str) -> bool {
    name.contains("__")
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-AGENT-014
    #[test]
    fn mcp_manager_starts_empty() {
        let mgr = McpManager::new();
        assert_eq!(mgr.connection_count(), 0);
        assert!(mgr.tool_schemas().is_empty());
        assert!(mgr.all_tools().is_empty());
    }

    // rtmx:req REQ-AGENT-014
    #[tokio::test]
    async fn connect_stdio_spawns_process() {
        // Use a simple command that implements a minimal MCP server.
        // We use a shell script that echoes the expected responses.
        let mut mgr = McpManager::new();

        // This test verifies that connect() attempts to spawn a process.
        // Using a nonexistent command should return an error.
        let config = McpServerConfig {
            name: "test-server".to_string(),
            transport: McpTransport::Stdio {
                command: "__nonexistent_mcp_server_binary__".to_string(),
                args: vec![],
                env: HashMap::new(),
            },
        };

        let result = mgr.connect(config).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Failed to spawn"),
            "Error should mention spawn failure: {err}"
        );
    }

    // rtmx:req REQ-AGENT-014
    #[tokio::test]
    async fn shutdown_clears_connections() {
        let mut mgr = McpManager::new();
        // After shutdown (even with no connections), state should be clean.
        mgr.shutdown().await;
        assert_eq!(mgr.connection_count(), 0);
    }

    // rtmx:req REQ-AGENT-014
    #[tokio::test]
    async fn sse_transport_returns_not_implemented() {
        let mut mgr = McpManager::new();
        let config = McpServerConfig {
            name: "sse-server".to_string(),
            transport: McpTransport::Sse {
                url: "http://localhost:8080".to_string(),
                headers: HashMap::new(),
            },
        };
        let result = mgr.connect(config).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("not yet implemented")
        );
    }

    // rtmx:req REQ-AGENT-023
    #[test]
    fn tool_schema_includes_server_prefix() {
        let tool = McpTool {
            server_name: "db".to_string(),
            name: "query".to_string(),
            description: "Run a database query".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "sql": {"type": "string"}
                }
            }),
        };
        let schema = tool.to_tool_schema();
        assert_eq!(schema.name, "db__query");
    }

    // rtmx:req REQ-AGENT-023
    #[test]
    fn tool_schema_preserves_description() {
        let tool = McpTool {
            server_name: "fs".to_string(),
            name: "read".to_string(),
            description: "Read a file from the filesystem".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        };
        let schema = tool.to_tool_schema();
        assert_eq!(schema.description, "Read a file from the filesystem");
    }

    // rtmx:req REQ-AGENT-023
    #[test]
    fn tool_schema_preserves_input_schema() {
        let input_schema = serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "encoding": {"type": "string", "default": "utf-8"}
            },
            "required": ["path"]
        });
        let tool = McpTool {
            server_name: "fs".to_string(),
            name: "read".to_string(),
            description: "Read file".to_string(),
            input_schema: input_schema.clone(),
        };
        let schema = tool.to_tool_schema();
        assert_eq!(schema.parameters, input_schema);
    }

    // rtmx:req REQ-AGENT-023
    #[tokio::test]
    async fn multiple_servers_tools_merged() {
        let mgr = McpManager {
            connections: vec![
                McpConnection {
                    server_name: "db".to_string(),
                    transport: mock_transport(),
                    tools: vec![
                        McpTool {
                            server_name: "db".to_string(),
                            name: "query".to_string(),
                            description: "Query DB".to_string(),
                            input_schema: serde_json::json!({
                                "type": "object"
                            }),
                        },
                        McpTool {
                            server_name: "db".to_string(),
                            name: "insert".to_string(),
                            description: "Insert row".to_string(),
                            input_schema: serde_json::json!({
                                "type": "object"
                            }),
                        },
                    ],
                },
                McpConnection {
                    server_name: "git".to_string(),
                    transport: mock_transport(),
                    tools: vec![McpTool {
                        server_name: "git".to_string(),
                        name: "status".to_string(),
                        description: "Git status".to_string(),
                        input_schema: serde_json::json!({
                            "type": "object"
                        }),
                    }],
                },
            ],
        };

        let schemas = mgr.tool_schemas();
        assert_eq!(schemas.len(), 3);
        let names: Vec<&str> = schemas.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"db__query"));
        assert!(names.contains(&"db__insert"));
        assert!(names.contains(&"git__status"));
    }

    // rtmx:req REQ-AGENT-024
    #[test]
    fn mcp_tool_risk_is_execute() {
        let tool = McpTool {
            server_name: "any".to_string(),
            name: "anything".to_string(),
            description: "Some tool".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        };
        // All MCP tools are StateMutating (highest risk in the current
        // ToolRisk enum) to ensure HITL approval is always required.
        assert_eq!(tool.risk(), ToolRisk::StateMutating);
    }

    // rtmx:req REQ-AGENT-024
    #[test]
    fn mcp_tool_requires_approval() {
        // StateMutating risk triggers HITL approval in the agent loop.
        // Verify that all MCP tools have this risk level.
        let tools = vec![
            McpTool {
                server_name: "s".to_string(),
                name: "read".to_string(),
                description: "".to_string(),
                input_schema: serde_json::json!({}),
            },
            McpTool {
                server_name: "s".to_string(),
                name: "write".to_string(),
                description: "".to_string(),
                input_schema: serde_json::json!({}),
            },
            McpTool {
                server_name: "s".to_string(),
                name: "delete".to_string(),
                description: "".to_string(),
                input_schema: serde_json::json!({}),
            },
        ];
        for tool in &tools {
            assert_eq!(
                tool.risk(),
                ToolRisk::StateMutating,
                "MCP tool '{}' should be StateMutating",
                tool.name
            );
        }
    }

    // rtmx:req REQ-AGENT-025
    #[test]
    fn large_mcp_output_is_truncated() {
        // Verify that truncate_output works on large strings
        // (the same function used in McpManager::execute).
        let large = "x".repeat(100_000);
        let result = truncate_output(&large);
        assert!(result.len() < large.len());
        assert!(result.ends_with("[output truncated at 64KB]"));
    }

    // rtmx:req REQ-AGENT-025
    #[test]
    fn small_mcp_output_passes_through() {
        let small = "Hello, world!";
        let result = truncate_output(small);
        assert_eq!(result, small);
    }

    // rtmx:req REQ-AGENT-022
    #[test]
    fn parse_qualified_name_splits_correctly() {
        let (server, tool) = parse_qualified_name("db__query").unwrap();
        assert_eq!(server, "db");
        assert_eq!(tool, "query");
    }

    // rtmx:req REQ-AGENT-022
    #[test]
    fn parse_qualified_name_returns_none_for_invalid() {
        assert!(parse_qualified_name("no_separator").is_none());
    }

    // rtmx:req REQ-AGENT-022
    #[test]
    fn is_mcp_tool_detects_qualified_names() {
        assert!(is_mcp_tool("db__query"));
        assert!(!is_mcp_tool("read_file"));
        assert!(!is_mcp_tool("grep"));
    }

    // rtmx:req REQ-AGENT-023
    #[test]
    fn qualified_name_format() {
        let tool = McpTool {
            server_name: "my-server".to_string(),
            name: "do-thing".to_string(),
            description: "".to_string(),
            input_schema: serde_json::json!({}),
        };
        assert_eq!(tool.qualified_name(), "my-server__do-thing");
    }

    // rtmx:req REQ-AGENT-014
    #[test]
    fn mcp_server_config_serializes() {
        let config = McpServerConfig {
            name: "test".to_string(),
            transport: McpTransport::Stdio {
                command: "node".to_string(),
                args: vec!["server.js".to_string()],
                env: HashMap::new(),
            },
        };
        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["name"], "test");
        assert!(
            json["transport"]["Stdio"]["command"]
                .as_str()
                .unwrap()
                .contains("node")
        );
    }

    // rtmx:req REQ-AGENT-014
    #[test]
    fn mcp_server_config_deserializes() {
        let json = serde_json::json!({
            "name": "my-server",
            "transport": {
                "Stdio": {
                    "command": "python",
                    "args": ["-m", "mcp_server"],
                    "env": {"DEBUG": "1"}
                }
            }
        });
        let config: McpServerConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config.name, "my-server");
        match config.transport {
            McpTransport::Stdio {
                command, args, env, ..
            } => {
                assert_eq!(command, "python");
                assert_eq!(args, vec!["-m", "mcp_server"]);
                assert_eq!(env.get("DEBUG").unwrap(), "1");
            }
            _ => panic!("Expected Stdio transport"),
        }
    }

    /// Helper: create a mock transport for tests that only need
    /// to inspect tool schemas (no actual I/O).
    fn mock_transport() -> ActiveTransport {
        // Spawn a trivial process just to have valid handles.
        // We use `cat` which will block on stdin -- we never
        // actually write to it in schema-only tests.
        let mut child = tokio::process::Command::new("cat")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("failed to spawn cat for mock transport");

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();

        // Kill the process immediately since we don't need it
        // (ignore errors since it may have already exited).
        let _ = child.start_kill();

        ActiveTransport::Stdio {
            child,
            stdin: BufWriter::new(stdin),
            stdout: BufReader::new(stdout),
            next_id: 1,
        }
    }
}
