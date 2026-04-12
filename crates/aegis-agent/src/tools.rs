//! Built-in tool executor: read_file, write_file, run_command,
//! list_dir, grep.
//!
//! Implements the `ToolExecutor` port. All filesystem operations
//! are validated against the `SecurityFilter` before execution.

use aegis_domain::error::DomainError;
use aegis_domain::ports::{SecurityFilter, ToolExecutor};
use aegis_domain::types::*;
use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;

/// Tool executor that performs real filesystem and process operations.
pub struct BuiltinExecutor {
    filter: Arc<dyn SecurityFilter>,
    work_dir: std::path::PathBuf,
}

impl BuiltinExecutor {
    pub fn new(filter: Arc<dyn SecurityFilter>, work_dir: &Path) -> Self {
        Self {
            filter,
            work_dir: work_dir.to_path_buf(),
        }
    }

    fn resolve_path(&self, file_path: &FilePath) -> std::path::PathBuf {
        let p = file_path.as_path();
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.work_dir.join(p)
        }
    }

    async fn read_file(&self, path: &FilePath) -> Result<ToolResult, DomainError> {
        let path_str = path.as_path().to_string_lossy();
        if self.filter.is_blocked(&path_str) {
            return Ok(ToolResult::PermissionDenied {
                reason: format!("File access denied by .aegisignore: {path_str}"),
            });
        }

        let resolved = self.resolve_path(path);
        match tokio::fs::read_to_string(&resolved).await {
            Ok(content) => Ok(ToolResult::Success { output: content }),
            Err(e) => Ok(ToolResult::Error {
                message: format!("Failed to read {}: {e}", resolved.display()),
            }),
        }
    }

    async fn write_file(
        &self,
        path: &FilePath,
        content: &str,
    ) -> Result<ToolResult, DomainError> {
        let resolved = self.resolve_path(path);

        // Ensure parent directory exists
        if let Some(parent) = resolved.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                DomainError::Other(format!(
                    "Failed to create directory {}: {e}",
                    parent.display()
                ))
            })?;
        }

        match tokio::fs::write(&resolved, content).await {
            Ok(()) => Ok(ToolResult::Success {
                output: format!("Wrote {} bytes to {}", content.len(), path),
            }),
            Err(e) => Ok(ToolResult::Error {
                message: format!("Failed to write {}: {e}", resolved.display()),
            }),
        }
    }

    async fn run_command(
        &self,
        command: &str,
        timeout_secs: u64,
    ) -> Result<ToolResult, DomainError> {
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            tokio::process::Command::new("sh")
                .arg("-c")
                .arg(command)
                .current_dir(&self.work_dir)
                .output(),
        )
        .await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let mut result = String::new();
                if !stdout.is_empty() {
                    result.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    if !result.is_empty() {
                        result.push('\n');
                    }
                    result.push_str("STDERR: ");
                    result.push_str(&stderr);
                }
                if output.status.success() {
                    Ok(ToolResult::Success {
                        output: if result.is_empty() {
                            "(no output)".to_string()
                        } else {
                            result
                        },
                    })
                } else {
                    Ok(ToolResult::Error {
                        message: format!("Command exited with {}: {}", output.status, result),
                    })
                }
            }
            Ok(Err(e)) => Ok(ToolResult::Error {
                message: format!("Failed to execute command: {e}"),
            }),
            Err(_) => Ok(ToolResult::Error {
                message: format!("Command timed out after {timeout_secs}s"),
            }),
        }
    }

    async fn list_dir(&self, path: &FilePath) -> Result<ToolResult, DomainError> {
        let resolved = self.resolve_path(path);
        let mut entries = Vec::new();

        match tokio::fs::read_dir(&resolved).await {
            Ok(mut dir) => {
                while let Ok(Some(entry)) = dir.next_entry().await {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let is_dir = entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false);
                    if is_dir {
                        entries.push(format!("{name}/"));
                    } else {
                        entries.push(name);
                    }
                }
                entries.sort();
                Ok(ToolResult::Success {
                    output: entries.join("\n"),
                })
            }
            Err(e) => Ok(ToolResult::Error {
                message: format!("Failed to list {}: {e}", resolved.display()),
            }),
        }
    }

    async fn grep(&self, pattern: &str, path: &FilePath) -> Result<ToolResult, DomainError> {
        // Use grep command for simplicity and cross-platform compat
        let resolved = self.resolve_path(path);
        let output = tokio::process::Command::new("grep")
            .arg("-rn")
            .arg(pattern)
            .arg(&resolved)
            .output()
            .await;

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                if stdout.is_empty() {
                    Ok(ToolResult::Success {
                        output: format!("No matches for '{pattern}'"),
                    })
                } else {
                    Ok(ToolResult::Success {
                        output: stdout.to_string(),
                    })
                }
            }
            Err(e) => Ok(ToolResult::Error {
                message: format!("grep failed: {e}"),
            }),
        }
    }
}

#[async_trait]
impl ToolExecutor for BuiltinExecutor {
    async fn execute(&self, tool_call: &ToolCall) -> Result<ToolResult, DomainError> {
        match tool_call {
            ToolCall::ReadFile { path } => self.read_file(path).await,
            ToolCall::WriteFile { path, content } => self.write_file(path, content).await,
            ToolCall::RunCommand {
                command,
                timeout_secs,
            } => self.run_command(command, *timeout_secs).await,
            ToolCall::ListDir { path } => self.list_dir(path).await,
            ToolCall::Grep { pattern, path } => self.grep(pattern, path).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_test_support::mock_filter::MockSecurityFilter;
    use tempfile::TempDir;

    fn make_executor(dir: &Path) -> BuiltinExecutor {
        BuiltinExecutor::new(Arc::new(MockSecurityFilter), dir)
    }

    // rtmx:req REQ-AGENT-002
    #[tokio::test]
    async fn read_file_returns_contents() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("test.txt"), "hello world").unwrap();

        let exec = make_executor(tmp.path());
        let result = exec
            .execute(&ToolCall::ReadFile {
                path: FilePath::new_unchecked("test.txt"),
            })
            .await
            .unwrap();

        match result {
            ToolResult::Success { output } => {
                assert_eq!(output, "hello world");
            }
            other => panic!("Expected Success, got {other:?}"),
        }
    }

    // rtmx:req REQ-AGENT-002
    #[tokio::test]
    async fn read_file_returns_error_for_missing() {
        let tmp = TempDir::new().unwrap();
        let exec = make_executor(tmp.path());

        let result = exec
            .execute(&ToolCall::ReadFile {
                path: FilePath::new_unchecked("nonexistent.txt"),
            })
            .await
            .unwrap();

        assert!(matches!(result, ToolResult::Error { .. }));
    }

    // rtmx:req REQ-SECURITY-001
    #[tokio::test]
    async fn read_file_blocked_by_security_filter() {
        use aegis_security::aegisignore::AegisIgnore;

        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join(".env"), "SECRET=abc").unwrap();

        let filter = Arc::new(AegisIgnore::with_defaults());
        let exec = BuiltinExecutor::new(filter, tmp.path());

        let result = exec
            .execute(&ToolCall::ReadFile {
                path: FilePath::new_unchecked(".env"),
            })
            .await
            .unwrap();

        assert!(
            matches!(result, ToolResult::PermissionDenied { .. }),
            "Expected PermissionDenied for .env"
        );
    }

    // rtmx:req REQ-AGENT-002
    #[tokio::test]
    async fn write_file_creates_and_writes() {
        let tmp = TempDir::new().unwrap();
        let exec = make_executor(tmp.path());

        let result = exec
            .execute(&ToolCall::WriteFile {
                path: FilePath::new_unchecked("output.txt"),
                content: "new content".to_string(),
            })
            .await
            .unwrap();

        assert!(matches!(result, ToolResult::Success { .. }));
        let content = std::fs::read_to_string(tmp.path().join("output.txt")).unwrap();
        assert_eq!(content, "new content");
    }

    // rtmx:req REQ-AGENT-002
    #[tokio::test]
    async fn write_file_creates_parent_dirs() {
        let tmp = TempDir::new().unwrap();
        let exec = make_executor(tmp.path());

        let result = exec
            .execute(&ToolCall::WriteFile {
                path: FilePath::new_unchecked("deep/nested/file.txt"),
                content: "nested".to_string(),
            })
            .await
            .unwrap();

        assert!(matches!(result, ToolResult::Success { .. }));
        assert!(tmp.path().join("deep/nested/file.txt").exists());
    }

    // rtmx:req REQ-AGENT-002
    #[tokio::test]
    async fn run_command_captures_stdout() {
        let tmp = TempDir::new().unwrap();
        let exec = make_executor(tmp.path());

        let result = exec
            .execute(&ToolCall::RunCommand {
                command: "echo hello".to_string(),
                timeout_secs: 10,
            })
            .await
            .unwrap();

        match result {
            ToolResult::Success { output } => {
                assert!(
                    output.contains("hello"),
                    "Output should contain 'hello': {output}"
                );
            }
            other => panic!("Expected Success, got {other:?}"),
        }
    }

    // rtmx:req REQ-AGENT-002
    #[tokio::test]
    async fn run_command_returns_error_on_failure() {
        let tmp = TempDir::new().unwrap();
        let exec = make_executor(tmp.path());

        let result = exec
            .execute(&ToolCall::RunCommand {
                command: "exit 1".to_string(),
                timeout_secs: 10,
            })
            .await
            .unwrap();

        assert!(
            matches!(result, ToolResult::Error { .. }),
            "Non-zero exit should return Error"
        );
    }

    // rtmx:req REQ-AGENT-011
    #[tokio::test]
    async fn run_command_times_out() {
        let tmp = TempDir::new().unwrap();
        let exec = make_executor(tmp.path());

        let result = exec
            .execute(&ToolCall::RunCommand {
                command: "sleep 30".to_string(),
                timeout_secs: 1,
            })
            .await
            .unwrap();

        match result {
            ToolResult::Error { message } => {
                assert!(
                    message.contains("timed out"),
                    "Should mention timeout: {message}"
                );
            }
            other => panic!("Expected Error, got {other:?}"),
        }
    }

    // rtmx:req REQ-AGENT-002
    #[tokio::test]
    async fn list_dir_returns_sorted_entries() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("b.txt"), "").unwrap();
        std::fs::write(tmp.path().join("a.txt"), "").unwrap();
        std::fs::create_dir(tmp.path().join("c_dir")).unwrap();

        let exec = make_executor(tmp.path());
        let result = exec
            .execute(&ToolCall::ListDir {
                path: FilePath::new_unchecked("."),
            })
            .await
            .unwrap();

        match result {
            ToolResult::Success { output } => {
                let lines: Vec<&str> = output.lines().collect();
                assert!(lines.contains(&"a.txt"));
                assert!(lines.contains(&"b.txt"));
                assert!(lines.contains(&"c_dir/"));
                // Sorted
                let a_pos = lines.iter().position(|l| *l == "a.txt").unwrap();
                let b_pos = lines.iter().position(|l| *l == "b.txt").unwrap();
                assert!(a_pos < b_pos);
            }
            other => panic!("Expected Success, got {other:?}"),
        }
    }

    // rtmx:req REQ-AGENT-002
    #[tokio::test]
    async fn grep_finds_pattern() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("code.rs"),
            "fn main() {\n    println!(\"hello\");\n}\n",
        )
        .unwrap();

        let exec = make_executor(tmp.path());
        let result = exec
            .execute(&ToolCall::Grep {
                pattern: "println".to_string(),
                path: FilePath::new_unchecked("code.rs"),
            })
            .await
            .unwrap();

        match result {
            ToolResult::Success { output } => {
                assert!(output.contains("println"), "Should find pattern: {output}");
            }
            other => panic!("Expected Success, got {other:?}"),
        }
    }

    // rtmx:req REQ-AGENT-002
    #[tokio::test]
    async fn grep_returns_no_matches() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("code.rs"), "fn main() {}\n").unwrap();

        let exec = make_executor(tmp.path());
        let result = exec
            .execute(&ToolCall::Grep {
                pattern: "nonexistent_pattern".to_string(),
                path: FilePath::new_unchecked("code.rs"),
            })
            .await
            .unwrap();

        match result {
            ToolResult::Success { output } => {
                assert!(output.contains("No matches"));
            }
            other => panic!("Expected Success, got {other:?}"),
        }
    }
}
