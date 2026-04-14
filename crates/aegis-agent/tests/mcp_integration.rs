//! Integration tests for MCP server connection, discovery, and execution.
//!
//! Uses a mock MCP server (shell script) that responds to JSON-RPC
//! over stdin/stdout with canned tool definitions and results.

use aegis_agent::mcp::{McpManager, McpServerConfig, McpTransport};
use std::collections::HashMap;

fn mock_server_path() -> String {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .join("tests/mock_mcp_server.sh")
        .to_string_lossy()
        .to_string()
}

// rtmx:req REQ-AGENT-022
#[tokio::test]
async fn mcp_stdio_connect_and_discover_tools() {
    let mut mgr = McpManager::new();
    let config = McpServerConfig {
        name: "mock".to_string(),
        transport: McpTransport::Stdio {
            command: "bash".to_string(),
            args: vec![mock_server_path()],
            env: HashMap::new(),
        },
    };

    let tools = mgr.connect(config).await.unwrap();
    assert_eq!(tools.len(), 1, "mock server should expose 1 tool");
    assert_eq!(tools[0].name, "echo");
    assert_eq!(tools[0].server_name, "mock");

    mgr.shutdown().await;
}

// rtmx:req REQ-AGENT-023
#[tokio::test]
async fn mcp_tool_schemas_merged_with_qualified_names() {
    let mut mgr = McpManager::new();
    let config = McpServerConfig {
        name: "mock".to_string(),
        transport: McpTransport::Stdio {
            command: "bash".to_string(),
            args: vec![mock_server_path()],
            env: HashMap::new(),
        },
    };

    mgr.connect(config).await.unwrap();
    let schemas = mgr.tool_schemas();
    assert_eq!(schemas.len(), 1);
    assert_eq!(schemas[0].name, "mock__echo");
    assert!(!schemas[0].description.is_empty());

    mgr.shutdown().await;
}

// rtmx:req REQ-AGENT-025
#[tokio::test]
async fn mcp_tool_execution_returns_result() {
    let mut mgr = McpManager::new();
    let config = McpServerConfig {
        name: "mock".to_string(),
        transport: McpTransport::Stdio {
            command: "bash".to_string(),
            args: vec![mock_server_path()],
            env: HashMap::new(),
        },
    };

    mgr.connect(config).await.unwrap();
    let result = mgr
        .execute("mock__echo", serde_json::json!({"message": "world"}))
        .await
        .unwrap();
    assert!(
        result.contains("echo: world"),
        "expected echo response, got: {result}"
    );

    mgr.shutdown().await;
}

// rtmx:req REQ-AGENT-022
#[tokio::test]
async fn mcp_connect_nonexistent_binary_returns_error() {
    let mut mgr = McpManager::new();
    let config = McpServerConfig {
        name: "bad".to_string(),
        transport: McpTransport::Stdio {
            command: "__no_such_binary__".to_string(),
            args: vec![],
            env: HashMap::new(),
        },
    };

    let result = mgr.connect(config).await;
    assert!(result.is_err());
}

// rtmx:req REQ-AGENT-014
#[tokio::test]
async fn mcp_execute_unknown_server_returns_error() {
    let mut mgr = McpManager::new();
    let result = mgr.execute("unknown__tool", serde_json::json!({})).await;
    assert!(result.is_err());
}

// rtmx:req REQ-AGENT-024
#[tokio::test]
async fn mcp_tools_are_state_mutating() {
    use aegis_domain::types::ToolRisk;

    let mut mgr = McpManager::new();
    let config = McpServerConfig {
        name: "mock".to_string(),
        transport: McpTransport::Stdio {
            command: "bash".to_string(),
            args: vec![mock_server_path()],
            env: HashMap::new(),
        },
    };

    let tools = mgr.connect(config).await.unwrap();
    for tool in &tools {
        assert_eq!(
            tool.risk(),
            ToolRisk::StateMutating,
            "all MCP tools must be StateMutating for HITL enforcement"
        );
    }

    mgr.shutdown().await;
}
