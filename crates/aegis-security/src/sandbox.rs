//! OS-level sandboxing for tool execution (REQ-SECURITY-003).
//!
//! Provides a policy-checking sandbox that restricts filesystem access and
//! command execution during tool calls. All tool execution in the agent loop
//! should route through `Sandbox::execute()`.
//!
//! ## Current implementation
//!
//! The sandbox currently operates as a **policy layer**: it validates filesystem
//! paths against an allow-list and rejects banned commands before delegating to
//! `std::process::Command`. No OS-level process isolation is applied yet.
//!
//! ## Future work
//!
//! - **Linux:** Integrate bubblewrap (`bwrap`) to create mount/network/PID
//!   namespaces around each command invocation.
//! - **macOS:** Integrate `sandbox-exec` (seatbelt profiles) to restrict
//!   filesystem and network access at the kernel level.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Banned command patterns that the sandbox refuses to execute.
///
/// This mirrors the pattern from `aegis-agent/src/banned_commands.rs` so
/// that the sandbox enforces the same safety rules independently.
const BANNED_PATTERNS: &[&str] = &[
    "rm -rf /",
    "rm -rf /*",
    "mkfs",
    "dd if=/dev/zero",
    "dd if=/dev/random",
    "dd if=/dev/urandom",
    ":(){ :|:& };:",
    ".() { .|.& }; .",
    "curl|sh",
    "curl | sh",
    "curl|bash",
    "curl | bash",
    "wget|sh",
    "wget | sh",
    "wget|bash",
    "wget | bash",
    "> /dev/sda",
    "> /dev/hda",
    "chmod -R 777 /",
    "chown -R",
    "mv / ",
    "dd of=/dev/sda",
    "dd of=/dev/hda",
];

/// Configuration for the sandbox policy.
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Paths with full read/write access.
    pub allowed_paths: Vec<PathBuf>,
    /// Paths with read-only access.
    pub readonly_paths: Vec<PathBuf>,
    /// Whether network egress is permitted.
    pub allow_network: bool,
    /// Working directory for command execution.
    pub working_dir: PathBuf,
}

impl Default for SandboxConfig {
    /// Default config: allows cwd and user home directory (read-only).
    fn default() -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let home = dirs_path_home();
        let mut readonly = Vec::new();
        if let Some(h) = home {
            readonly.push(h);
        }
        Self {
            allowed_paths: vec![cwd.clone()],
            readonly_paths: readonly,
            allow_network: false,
            working_dir: cwd,
        }
    }
}

/// Best-effort home directory lookup without adding a dependency.
fn dirs_path_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Result of a sandboxed command execution.
#[derive(Debug, Clone)]
pub struct SandboxResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Errors produced by the sandbox.
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("path access denied by sandbox policy: {path}")]
    PathDenied { path: String },

    #[error("command is banned by sandbox policy: {command}")]
    BannedCommand { command: String },

    #[error("command execution failed: {0}")]
    ExecutionError(#[from] io::Error),
}

/// Policy-enforcing sandbox for tool execution.
#[derive(Debug, Clone)]
pub struct Sandbox {
    config: SandboxConfig,
}

impl Sandbox {
    pub fn new(config: SandboxConfig) -> Self {
        Self { config }
    }

    /// Execute a command inside the sandbox.
    ///
    /// Validates that the command is not banned, then runs it via
    /// `std::process::Command` with the configured working directory.
    pub fn execute(&self, command: &str, args: &[&str]) -> Result<SandboxResult, SandboxError> {
        // Reconstruct the full command string for banned-pattern checking.
        let full_command = if args.is_empty() {
            command.to_string()
        } else {
            format!("{} {}", command, args.join(" "))
        };

        if is_command_banned(&full_command) {
            return Err(SandboxError::BannedCommand {
                command: full_command,
            });
        }

        let output = Command::new(command)
            .args(args)
            .current_dir(&self.config.working_dir)
            .output()?;

        Ok(SandboxResult {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    /// Check if a path is accessible under the sandbox policy.
    ///
    /// Canonicalizes the path first to defeat traversal attacks (e.g.
    /// `../../../etc/passwd`). If the path does not exist on disk, falls
    /// back to lexical normalization so the policy still applies.
    pub fn is_path_allowed(&self, path: &Path, write: bool) -> bool {
        let resolved = canonicalize_or_normalize(path);

        // Check read/write allowed paths first.
        for allowed in &self.config.allowed_paths {
            let allowed_canon = canonicalize_or_normalize(allowed);
            if resolved.starts_with(&allowed_canon) {
                return true;
            }
        }

        // If the caller only needs read access, also check readonly paths.
        if !write {
            for ro in &self.config.readonly_paths {
                let ro_canon = canonicalize_or_normalize(ro);
                if resolved.starts_with(&ro_canon) {
                    return true;
                }
            }
        }

        false
    }
}

/// Canonicalize a path, falling back to lexical normalization if the path
/// does not exist on disk.
fn canonicalize_or_normalize(path: &Path) -> PathBuf {
    if let Ok(canon) = std::fs::canonicalize(path) {
        return canon;
    }
    lexical_normalize(path)
}

/// Lexical normalization: resolve `.` and `..` components without touching
/// the filesystem. Used as fallback when the path does not exist.
fn lexical_normalize(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                // Pop the last normal component, if any.
                if matches!(components.last(), Some(std::path::Component::Normal(_))) {
                    components.pop();
                } else {
                    components.push(component);
                }
            }
            std::path::Component::CurDir => {
                // Skip `.` components.
            }
            _ => {
                components.push(component);
            }
        }
    }
    components.iter().collect()
}

/// Check whether a command string matches any banned pattern.
///
/// Matching is case-insensitive and whitespace-normalized (consecutive
/// whitespace collapsed to single spaces).
fn is_command_banned(command: &str) -> bool {
    let normalized = normalize_whitespace(command);
    let lower = normalized.to_lowercase();

    if BANNED_PATTERNS
        .iter()
        .any(|pattern| lower.contains(&normalize_whitespace(pattern).to_lowercase()))
    {
        return true;
    }

    // curl/wget piped to shell with a URL in between.
    let has_download = lower.starts_with("curl ") || lower.starts_with("wget ");
    let has_pipe_shell = lower.contains("|sh")
        || lower.contains("| sh")
        || lower.contains("|bash")
        || lower.contains("| bash");
    if has_download && has_pipe_shell {
        return true;
    }

    false
}

fn normalize_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // @req REQ-SECURITY-003
    #[test]
    fn allowed_path_returns_true_for_child() {
        let config = SandboxConfig {
            allowed_paths: vec![PathBuf::from("/tmp/sandbox_test")],
            readonly_paths: vec![],
            allow_network: false,
            working_dir: PathBuf::from("/tmp"),
        };
        let sandbox = Sandbox::new(config);
        assert!(sandbox.is_path_allowed(Path::new("/tmp/sandbox_test/foo.txt"), false));
        assert!(sandbox.is_path_allowed(Path::new("/tmp/sandbox_test/foo.txt"), true));
    }

    // @req REQ-SECURITY-003
    #[test]
    fn disallowed_path_returns_false() {
        let config = SandboxConfig {
            allowed_paths: vec![PathBuf::from("/tmp/sandbox_test")],
            readonly_paths: vec![],
            allow_network: false,
            working_dir: PathBuf::from("/tmp"),
        };
        let sandbox = Sandbox::new(config);
        assert!(!sandbox.is_path_allowed(Path::new("/etc/passwd"), false));
        assert!(!sandbox.is_path_allowed(Path::new("/etc/passwd"), true));
    }

    // @req REQ-SECURITY-003
    #[test]
    fn readonly_path_denies_write() {
        let config = SandboxConfig {
            allowed_paths: vec![],
            readonly_paths: vec![PathBuf::from("/tmp/readonly_area")],
            allow_network: false,
            working_dir: PathBuf::from("/tmp"),
        };
        let sandbox = Sandbox::new(config);
        // Read access should be allowed.
        assert!(sandbox.is_path_allowed(Path::new("/tmp/readonly_area/data.txt"), false));
        // Write access should be denied.
        assert!(!sandbox.is_path_allowed(Path::new("/tmp/readonly_area/data.txt"), true));
    }

    // @req REQ-SECURITY-003
    #[test]
    fn default_config_allows_cwd() {
        let config = SandboxConfig::default();
        let cwd = std::env::current_dir().expect("cwd");
        let sandbox = Sandbox::new(config);
        let child = cwd.join("some_file.rs");
        assert!(sandbox.is_path_allowed(&child, true));
    }

    // @req REQ-SECURITY-003
    #[test]
    fn default_config_allows_home_readonly() {
        let config = SandboxConfig::default();
        let sandbox = Sandbox::new(config);
        if let Some(home) = dirs_path_home() {
            let child = home.join(".config/some_config");
            // Read should be allowed via readonly_paths.
            assert!(sandbox.is_path_allowed(&child, false));
            // Write should only be allowed if home is under cwd
            // (unlikely), otherwise denied.
            let cwd = std::env::current_dir().unwrap();
            if !home.starts_with(&cwd) {
                assert!(!sandbox.is_path_allowed(&child, true));
            }
        }
    }

    // @req REQ-SECURITY-003
    #[test]
    fn path_traversal_is_blocked() {
        // Create a real temp directory so canonicalization works.
        let tmpdir = std::env::temp_dir().join("aegis_sandbox_test_traversal");
        let _ = fs::create_dir_all(&tmpdir);

        let config = SandboxConfig {
            allowed_paths: vec![tmpdir.clone()],
            readonly_paths: vec![],
            allow_network: false,
            working_dir: tmpdir.clone(),
        };
        let sandbox = Sandbox::new(config);

        // Attempt to escape via `..`.
        let traversal = tmpdir.join("subdir/../../../etc/passwd");
        assert!(
            !sandbox.is_path_allowed(&traversal, false),
            "traversal path should be blocked: {:?}",
            traversal
        );

        let _ = fs::remove_dir_all(&tmpdir);
    }

    // @req REQ-SECURITY-003
    #[test]
    fn symlink_resolution_blocks_escape() {
        // Create a temp directory with a symlink that points outside.
        let tmpdir = std::env::temp_dir().join("aegis_sandbox_test_symlink");
        let _ = fs::create_dir_all(&tmpdir);
        let link_path = tmpdir.join("escape_link");

        // Remove stale link from previous runs.
        let _ = fs::remove_file(&link_path);

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            // Symlink pointing to /etc.
            let _ = symlink("/etc", &link_path);
        }

        let config = SandboxConfig {
            allowed_paths: vec![tmpdir.clone()],
            readonly_paths: vec![],
            allow_network: false,
            working_dir: tmpdir.clone(),
        };
        let sandbox = Sandbox::new(config);

        // The symlink lives inside the allowed dir, but resolves
        // outside it.
        if link_path.exists() {
            assert!(
                !sandbox.is_path_allowed(&link_path.join("passwd"), false),
                "symlink escape should be blocked"
            );
        }

        let _ = fs::remove_dir_all(&tmpdir);
    }

    // @req REQ-SECURITY-003
    #[test]
    fn banned_command_is_rejected() {
        let config = SandboxConfig {
            allowed_paths: vec![],
            readonly_paths: vec![],
            allow_network: false,
            working_dir: PathBuf::from("/tmp"),
        };
        let sandbox = Sandbox::new(config);
        let result = sandbox.execute("rm", &["-rf", "/"]);
        assert!(result.is_err());
        assert!(matches!(result, Err(SandboxError::BannedCommand { .. })));
    }

    // @req REQ-SECURITY-003
    #[test]
    fn safe_command_executes() {
        let config = SandboxConfig {
            allowed_paths: vec![],
            readonly_paths: vec![],
            allow_network: false,
            working_dir: PathBuf::from("/tmp"),
        };
        let sandbox = Sandbox::new(config);
        let result = sandbox.execute("echo", &["hello"]);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.stdout.trim(), "hello");
        assert_eq!(output.exit_code, 0);
    }

    // @req REQ-SECURITY-003
    #[test]
    fn lexical_normalize_resolves_dotdot() {
        let normalized = lexical_normalize(Path::new("/a/b/../c"));
        assert_eq!(normalized, PathBuf::from("/a/c"));
    }

    // @req REQ-SECURITY-003
    #[test]
    fn lexical_normalize_resolves_dot() {
        let normalized = lexical_normalize(Path::new("/a/./b/./c"));
        assert_eq!(normalized, PathBuf::from("/a/b/c"));
    }

    // @req REQ-SECURITY-003
    #[test]
    fn is_command_banned_case_insensitive() {
        assert!(is_command_banned("RM -RF /"));
        assert!(is_command_banned("MKFS /dev/sda"));
    }

    // @req REQ-SECURITY-003
    #[test]
    fn is_command_banned_whitespace_normalized() {
        assert!(is_command_banned("rm   -rf   /"));
        assert!(is_command_banned("dd   if=/dev/zero   of=disk"));
    }

    // @req REQ-SECURITY-003
    #[test]
    fn safe_commands_not_banned() {
        assert!(!is_command_banned("ls -la"));
        assert!(!is_command_banned("cargo test"));
        assert!(!is_command_banned("echo hello"));
    }
}
