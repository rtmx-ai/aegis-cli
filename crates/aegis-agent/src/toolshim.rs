//! ToolShim: enables tool calling on models without native function_call/tool_use.
//!
//! Local models (Ollama, vLLM) often lack structured tool-calling APIs.
//! The shim works by:
//! 1. Injecting a system prompt describing available tools and expected JSON format
//! 2. Parsing the model's plain-text response to extract tool calls from JSON blocks

use aegis_domain::ports::ToolSchema;
use aegis_domain::types::{FilePath, ToolCall};

/// Build a system prompt section that describes the available tools and the
/// expected JSON response format for models without native tool calling.
pub fn build_toolshim_prompt(tools: &[ToolSchema]) -> String {
    let mut prompt = String::from(
        "You have access to the following tools. To use a tool, respond with a JSON \
         object in the following format:\n\n\
         ```json\n{\"tool\": \"tool_name\", \"arguments\": {\"arg1\": \"value1\"}}\n```\n\n\
         If you do not need to use a tool, respond with plain text.\n\n\
         Available tools:\n",
    );

    for tool in tools {
        prompt.push_str(&format!(
            "\n- **{}**: {}\n  Parameters: {}\n",
            tool.name, tool.description, tool.parameters
        ));
    }

    prompt
}

/// Parse a tool call from the model's text response.
///
/// Looks for JSON blocks (fenced in triple backticks or bare `{...}` objects)
/// and attempts to extract a `{"tool": "...", "arguments": {...}}` structure.
/// Returns `None` if the response is plain text with no tool invocation.
pub fn parse_toolshim_response(text: &str) -> Option<ToolCall> {
    // Try fenced code blocks first, then bare JSON objects
    let json_str = extract_fenced_json(text).or_else(|| extract_bare_json(text))?;

    let parsed: serde_json::Value = serde_json::from_str(&json_str).ok()?;
    let obj = parsed.as_object()?;

    let tool_name = obj.get("tool")?.as_str()?;
    let args = obj.get("arguments")?.as_object()?;

    match tool_name {
        "read_file" => {
            let path = args.get("path")?.as_str()?;
            Some(ToolCall::ReadFile {
                path: FilePath::new_unchecked(path),
            })
        }
        "write_file" => {
            let path = args.get("path")?.as_str()?;
            let content = args.get("content")?.as_str()?;
            Some(ToolCall::WriteFile {
                path: FilePath::new_unchecked(path),
                content: content.to_string(),
            })
        }
        "run_command" => {
            let command = args.get("command")?.as_str()?;
            let timeout_secs = args
                .get("timeout_secs")
                .and_then(|v| v.as_u64())
                .unwrap_or(30);
            Some(ToolCall::RunCommand {
                command: command.to_string(),
                timeout_secs,
            })
        }
        "list_dir" => {
            let path = args.get("path")?.as_str()?;
            Some(ToolCall::ListDir {
                path: FilePath::new_unchecked(path),
            })
        }
        "grep" => {
            let pattern = args.get("pattern")?.as_str()?;
            let path = args.get("path")?.as_str()?;
            Some(ToolCall::Grep {
                pattern: pattern.to_string(),
                path: FilePath::new_unchecked(path),
            })
        }
        _ => None,
    }
}

/// Return the text before any JSON tool-call block.
///
/// If the response contains a fenced code block or bare JSON object,
/// this returns everything before it (trimmed). If no JSON is found,
/// returns the full text unchanged.
pub fn extract_preamble(text: &str) -> String {
    // Check for fenced code block first
    if let Some(pos) = text.find("```") {
        return text[..pos].trim().to_string();
    }
    // Check for bare JSON object
    if let Some(pos) = text.find('{') {
        return text[..pos].trim().to_string();
    }
    text.to_string()
}

/// Extract JSON from a fenced code block (```json ... ``` or ``` ... ```).
fn extract_fenced_json(text: &str) -> Option<String> {
    // Find opening fence
    let start_marker_pos = text.find("```")?;
    let after_fence = &text[start_marker_pos + 3..];

    // Skip optional language tag (e.g., "json")
    let content_start = after_fence.find('\n')? + 1;
    let content = &after_fence[content_start..];

    // Find closing fence
    let end_pos = content.find("```")?;
    let json_str = content[..end_pos].trim();

    if json_str.is_empty() {
        return None;
    }

    Some(json_str.to_string())
}

/// Extract a bare JSON object from text (first `{` to its matching `}`).
fn extract_bare_json(text: &str) -> Option<String> {
    let start = text.find('{')?;
    let mut depth = 0i32;
    let mut end = start;

    for (i, ch) in text[start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = start + i;
                    break;
                }
            }
            _ => {}
        }
    }

    if depth != 0 {
        return None;
    }

    Some(text[start..=end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_domain::ports::ToolSchema;

    fn sample_tools() -> Vec<ToolSchema> {
        vec![
            ToolSchema {
                name: "read_file".to_string(),
                description: "Read a file".to_string(),
                parameters: serde_json::json!({"path": "string"}),
            },
            ToolSchema {
                name: "write_file".to_string(),
                description: "Write a file".to_string(),
                parameters: serde_json::json!({"path": "string", "content": "string"}),
            },
            ToolSchema {
                name: "run_command".to_string(),
                description: "Run a shell command".to_string(),
                parameters: serde_json::json!({"command": "string"}),
            },
            ToolSchema {
                name: "list_dir".to_string(),
                description: "List directory contents".to_string(),
                parameters: serde_json::json!({"path": "string"}),
            },
            ToolSchema {
                name: "grep".to_string(),
                description: "Search for a pattern".to_string(),
                parameters: serde_json::json!({"pattern": "string", "path": "string"}),
            },
        ]
    }

    // rtmx:req REQ-AGENT-003
    #[test]
    fn prompt_includes_all_tool_names() {
        let tools = sample_tools();
        let prompt = build_toolshim_prompt(&tools);

        for tool in &tools {
            assert!(
                prompt.contains(&tool.name),
                "Prompt should contain tool name '{}': {prompt}",
                tool.name
            );
        }
    }

    // rtmx:req REQ-AGENT-003
    #[test]
    fn parse_read_file_from_json_block() {
        let text = r#"I'll read that file for you.

```json
{"tool": "read_file", "arguments": {"path": "src/main.rs"}}
```"#;

        let result = parse_toolshim_response(text);
        assert!(result.is_some(), "Should parse read_file tool call");
        match result.unwrap() {
            ToolCall::ReadFile { path } => {
                assert_eq!(path.as_path().to_str().unwrap(), "src/main.rs");
            }
            other => panic!("Expected ReadFile, got {other:?}"),
        }
    }

    // rtmx:req REQ-AGENT-003
    #[test]
    fn parse_write_file_from_json_block() {
        let text = r#"Sure, I'll create that file.

```json
{"tool": "write_file", "arguments": {"path": "hello.txt", "content": "Hello, world!"}}
```"#;

        let result = parse_toolshim_response(text);
        assert!(result.is_some(), "Should parse write_file tool call");
        match result.unwrap() {
            ToolCall::WriteFile { path, content } => {
                assert_eq!(path.as_path().to_str().unwrap(), "hello.txt");
                assert_eq!(content, "Hello, world!");
            }
            other => panic!("Expected WriteFile, got {other:?}"),
        }
    }

    // rtmx:req REQ-AGENT-003
    #[test]
    fn parse_run_command_from_json_block() {
        let text = r#"Let me run that command.

```json
{"tool": "run_command", "arguments": {"command": "cargo test", "timeout_secs": 60}}
```"#;

        let result = parse_toolshim_response(text);
        assert!(result.is_some(), "Should parse run_command tool call");
        match result.unwrap() {
            ToolCall::RunCommand {
                command,
                timeout_secs,
            } => {
                assert_eq!(command, "cargo test");
                assert_eq!(timeout_secs, 60);
            }
            other => panic!("Expected RunCommand, got {other:?}"),
        }
    }

    // rtmx:req REQ-AGENT-003
    #[test]
    fn parse_list_dir_from_bare_json() {
        let text = r#"Here are the files: {"tool": "list_dir", "arguments": {"path": "src"}}"#;

        let result = parse_toolshim_response(text);
        assert!(result.is_some(), "Should parse list_dir from bare JSON");
        match result.unwrap() {
            ToolCall::ListDir { path } => {
                assert_eq!(path.as_path().to_str().unwrap(), "src");
            }
            other => panic!("Expected ListDir, got {other:?}"),
        }
    }

    // rtmx:req REQ-AGENT-003
    #[test]
    fn parse_grep_from_json_block() {
        let text = r#"```json
{"tool": "grep", "arguments": {"pattern": "TODO", "path": "src"}}
```"#;

        let result = parse_toolshim_response(text);
        assert!(result.is_some(), "Should parse grep tool call");
        match result.unwrap() {
            ToolCall::Grep { pattern, path } => {
                assert_eq!(pattern, "TODO");
                assert_eq!(path.as_path().to_str().unwrap(), "src");
            }
            other => panic!("Expected Grep, got {other:?}"),
        }
    }

    // rtmx:req REQ-AGENT-003
    #[test]
    fn plain_text_returns_none() {
        let text = "The answer to your question is 42. No tools needed here.";
        assert!(
            parse_toolshim_response(text).is_none(),
            "Plain text should return None"
        );
    }

    // rtmx:req REQ-AGENT-003
    #[test]
    fn malformed_json_returns_none() {
        let text = r#"```json
{"tool": "read_file", "arguments": {"path":
```"#;

        assert!(
            parse_toolshim_response(text).is_none(),
            "Malformed JSON should return None"
        );
    }

    // rtmx:req REQ-AGENT-003
    #[test]
    fn json_without_tool_field_returns_none() {
        let text = r#"```json
{"name": "read_file", "arguments": {"path": "src/main.rs"}}
```"#;

        assert!(
            parse_toolshim_response(text).is_none(),
            "JSON without 'tool' field should return None"
        );
    }

    // rtmx:req REQ-AGENT-003
    #[test]
    fn unknown_tool_name_returns_none() {
        let text = r#"```json
{"tool": "delete_everything", "arguments": {"confirm": true}}
```"#;

        assert!(
            parse_toolshim_response(text).is_none(),
            "Unknown tool name should return None"
        );
    }

    // rtmx:req REQ-AGENT-003
    #[test]
    fn run_command_defaults_timeout_when_omitted() {
        let text = r#"{"tool": "run_command", "arguments": {"command": "ls"}}"#;

        let result = parse_toolshim_response(text);
        assert!(result.is_some());
        match result.unwrap() {
            ToolCall::RunCommand { timeout_secs, .. } => {
                assert_eq!(timeout_secs, 30, "Should default to 30s timeout");
            }
            other => panic!("Expected RunCommand, got {other:?}"),
        }
    }

    // rtmx:req REQ-AGENT-003
    #[test]
    fn extract_preamble_from_fenced_block() {
        let text = "I'll read that file for you.\n\n```json\n{\"tool\": \"read_file\"}\n```";
        assert_eq!(extract_preamble(text), "I'll read that file for you.");
    }

    // rtmx:req REQ-AGENT-003
    #[test]
    fn extract_preamble_from_bare_json() {
        let text = r#"Here: {"tool": "list_dir", "arguments": {"path": "."}}"#;
        assert_eq!(extract_preamble(text), "Here:");
    }

    // rtmx:req REQ-AGENT-003
    #[test]
    fn extract_preamble_plain_text_unchanged() {
        let text = "No tools needed here.";
        assert_eq!(extract_preamble(text), "No tools needed here.");
    }

    // rtmx:req REQ-AGENT-003
    #[test]
    fn extract_preamble_bare_json_only() {
        let text = r#"{"tool": "list_dir", "arguments": {"path": "."}}"#;
        assert_eq!(extract_preamble(text), "");
    }
}
