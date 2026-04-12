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

/// Default maximum file read size: 10 MB.
const DEFAULT_MAX_FILE_READ_BYTES: usize = 10 * 1024 * 1024;

/// Default maximum process memory: 512 MB.
const DEFAULT_MAX_PROCESS_MEMORY_BYTES: usize = 512 * 1024 * 1024;

/// Default environment variable allowlist for process isolation.
const DEFAULT_ENV_ALLOWLIST: &[&str] = &["PATH", "HOME", "USER", "TERM"];

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
    /// Maximum file size in bytes that may be read (REQ-SECURITY-008).
    pub max_file_read_bytes: usize,
    /// Maximum process memory in bytes (REQ-SECURITY-008).
    pub max_process_memory_bytes: Option<usize>,
    /// Environment variables to pass through when `inherit_env` is false
    /// (REQ-SECURITY-009).
    pub env_allowlist: Vec<String>,
    /// If false, only allowlisted env vars are passed to child processes
    /// (REQ-SECURITY-009).
    pub inherit_env: bool,
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
            max_file_read_bytes: DEFAULT_MAX_FILE_READ_BYTES,
            max_process_memory_bytes: Some(DEFAULT_MAX_PROCESS_MEMORY_BYTES),
            env_allowlist: DEFAULT_ENV_ALLOWLIST
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            inherit_env: false,
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

    #[error("file too large: {path} is {size} bytes, limit is {limit} bytes")]
    FileTooLarge {
        path: PathBuf,
        size: u64,
        limit: usize,
    },

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

    /// Check whether a file's size is within the configured limit
    /// (REQ-SECURITY-008).
    ///
    /// Returns `Ok(())` if the file size is at or below `max_file_read_bytes`,
    /// or `Err(SandboxError::FileTooLarge)` otherwise.
    pub fn check_file_size(&self, path: &Path) -> Result<(), SandboxError> {
        let metadata = std::fs::metadata(path)?;
        let size = metadata.len();
        if size > self.config.max_file_read_bytes as u64 {
            return Err(SandboxError::FileTooLarge {
                path: path.to_path_buf(),
                size,
                limit: self.config.max_file_read_bytes,
            });
        }
        Ok(())
    }

    /// Execute a command inside the sandbox (REQ-SECURITY-009).
    ///
    /// Validates that the command is not banned, then runs it via a fresh
    /// `std::process::Command` with the configured working directory. When
    /// `inherit_env` is false, only allowlisted environment variables are
    /// passed to the child process.
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

        // Each execution gets a fresh Command -- no state from previous calls.
        let mut cmd = Command::new(command);
        cmd.args(args);
        cmd.current_dir(&self.config.working_dir);

        // Process isolation: clear environment and only pass allowlisted vars
        // (REQ-SECURITY-009).
        if !self.config.inherit_env {
            cmd.env_clear();
            for key in &self.config.env_allowlist {
                if let Ok(val) = std::env::var(key) {
                    cmd.env(key, val);
                }
            }
        }

        let output = cmd.output()?;

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
        return strip_unc_prefix(&canon);
    }
    lexical_normalize(path)
}

/// Strip the `\\?\` UNC prefix that Windows canonicalization adds.
/// Without this, `starts_with` comparisons fail because `\\?\C:\foo`
/// does not start with `C:\foo`.
fn strip_unc_prefix(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(stripped) => PathBuf::from(stripped),
        None => path.to_path_buf(),
    }
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
    use std::io::Write;

    /// Build a test config with sane defaults for the new fields.
    fn test_config(
        allowed_paths: Vec<PathBuf>,
        readonly_paths: Vec<PathBuf>,
        working_dir: PathBuf,
    ) -> SandboxConfig {
        SandboxConfig {
            allowed_paths,
            readonly_paths,
            allow_network: false,
            working_dir,
            max_file_read_bytes: DEFAULT_MAX_FILE_READ_BYTES,
            max_process_memory_bytes: Some(DEFAULT_MAX_PROCESS_MEMORY_BYTES),
            env_allowlist: DEFAULT_ENV_ALLOWLIST
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            inherit_env: true, // existing tests expect full env inheritance
        }
    }

    // @req REQ-SECURITY-003
    #[test]
    fn allowed_path_returns_true_for_child() {
        let config = test_config(
            vec![PathBuf::from("/tmp/sandbox_test")],
            vec![],
            PathBuf::from("/tmp"),
        );
        let sandbox = Sandbox::new(config);
        assert!(sandbox.is_path_allowed(Path::new("/tmp/sandbox_test/foo.txt"), false));
        assert!(sandbox.is_path_allowed(Path::new("/tmp/sandbox_test/foo.txt"), true));
    }

    // @req REQ-SECURITY-003
    #[test]
    fn disallowed_path_returns_false() {
        let config = test_config(
            vec![PathBuf::from("/tmp/sandbox_test")],
            vec![],
            PathBuf::from("/tmp"),
        );
        let sandbox = Sandbox::new(config);
        assert!(!sandbox.is_path_allowed(Path::new("/etc/passwd"), false));
        assert!(!sandbox.is_path_allowed(Path::new("/etc/passwd"), true));
    }

    // @req REQ-SECURITY-003
    #[test]
    fn readonly_path_denies_write() {
        let config = test_config(
            vec![],
            vec![PathBuf::from("/tmp/readonly_area")],
            PathBuf::from("/tmp"),
        );
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

        let config = test_config(vec![tmpdir.clone()], vec![], tmpdir.clone());
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

        let config = test_config(vec![tmpdir.clone()], vec![], tmpdir.clone());
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
        let config = test_config(vec![], vec![], PathBuf::from("/tmp"));
        let sandbox = Sandbox::new(config);
        let result = sandbox.execute("rm", &["-rf", "/"]);
        assert!(result.is_err());
        assert!(matches!(result, Err(SandboxError::BannedCommand { .. })));
    }

    // @req REQ-SECURITY-003
    #[test]
    fn safe_command_executes() {
        let dir = std::env::temp_dir();
        let config = test_config(vec![], vec![], dir);
        let sandbox = Sandbox::new(config);
        // Use a command that works on both Unix and Windows
        #[cfg(unix)]
        let result = sandbox.execute("echo", &["hello"]);
        #[cfg(windows)]
        let result = sandbox.execute("cmd", &["/C", "echo", "hello"]);
        assert!(result.is_ok(), "execute failed: {:?}", result.err());
        let output = result.unwrap();
        assert!(output.stdout.trim().contains("hello"));
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

    // --- REQ-SECURITY-008: File size and memory limits ---

    // @req REQ-SECURITY-008
    #[test]
    fn default_max_file_read_bytes_is_10mb() {
        let config = SandboxConfig::default();
        assert_eq!(config.max_file_read_bytes, 10 * 1024 * 1024);
    }

    // @req REQ-SECURITY-008
    #[test]
    fn default_max_process_memory_is_512mb() {
        let config = SandboxConfig::default();
        assert_eq!(config.max_process_memory_bytes, Some(512 * 1024 * 1024));
    }

    // @req REQ-SECURITY-008
    #[test]
    fn check_file_size_ok_for_small_file() {
        let tmpdir = std::env::temp_dir().join("aegis_sandbox_filesize");
        let _ = fs::create_dir_all(&tmpdir);
        let small_file = tmpdir.join("small.txt");
        {
            let mut f = fs::File::create(&small_file).expect("create small file");
            f.write_all(b"hello world").expect("write small file");
        }

        let config = SandboxConfig {
            max_file_read_bytes: 1024,
            ..SandboxConfig::default()
        };
        let sandbox = Sandbox::new(config);
        assert!(sandbox.check_file_size(&small_file).is_ok());

        let _ = fs::remove_dir_all(&tmpdir);
    }

    // @req REQ-SECURITY-008
    #[test]
    fn check_file_size_returns_file_too_large() {
        let tmpdir = std::env::temp_dir().join("aegis_sandbox_filesize_large");
        let _ = fs::create_dir_all(&tmpdir);
        let large_file = tmpdir.join("large.bin");
        {
            let mut f = fs::File::create(&large_file).expect("create large file");
            // Write 2048 bytes but set limit to 1024.
            let data = vec![0u8; 2048];
            f.write_all(&data).expect("write large file");
        }

        let config = SandboxConfig {
            max_file_read_bytes: 1024,
            ..SandboxConfig::default()
        };
        let sandbox = Sandbox::new(config);
        let result = sandbox.check_file_size(&large_file);
        assert!(result.is_err());
        match result.unwrap_err() {
            SandboxError::FileTooLarge { path, size, limit } => {
                assert_eq!(path, large_file);
                assert_eq!(size, 2048);
                assert_eq!(limit, 1024);
            }
            other => {
                panic!("expected FileTooLarge, got: {:?}", other)
            }
        }

        let _ = fs::remove_dir_all(&tmpdir);
    }

    // @req REQ-SECURITY-008
    #[test]
    fn file_too_large_error_includes_path_size_limit() {
        let err = SandboxError::FileTooLarge {
            path: PathBuf::from("/some/file.bin"),
            size: 20_000_000,
            limit: 10_485_760,
        };
        let msg = err.to_string();
        assert!(msg.contains("/some/file.bin"), "error should contain path");
        assert!(msg.contains("20000000"), "error should contain size");
        assert!(msg.contains("10485760"), "error should contain limit");
    }

    // --- REQ-SECURITY-009: Process isolation per tool invocation ---

    // @req REQ-SECURITY-009
    #[test]
    fn default_inherit_env_is_false() {
        let config = SandboxConfig::default();
        assert!(!config.inherit_env);
    }

    // @req REQ-SECURITY-009
    #[test]
    fn default_env_allowlist_includes_path() {
        let config = SandboxConfig::default();
        assert!(
            config.env_allowlist.contains(&"PATH".to_string()),
            "default allowlist should include PATH"
        );
    }

    // @req REQ-SECURITY-009
    #[test]
    fn default_env_allowlist_includes_expected_vars() {
        let config = SandboxConfig::default();
        for var in &["PATH", "HOME", "USER", "TERM"] {
            assert!(
                config.env_allowlist.contains(&var.to_string()),
                "default allowlist should include {}",
                var
            );
        }
    }

    // @req REQ-SECURITY-009
    #[cfg(unix)]
    #[test]
    fn execute_with_inherit_env_false_strips_env_vars() {
        let dir = std::env::temp_dir();
        // Set a custom env var that should NOT be passed through.
        unsafe {
            std::env::set_var("AEGIS_TEST_SECRET_009", "leaked");
        }

        let config = SandboxConfig {
            inherit_env: false,
            env_allowlist: vec!["PATH".to_string()],
            working_dir: dir,
            ..SandboxConfig::default()
        };
        let sandbox = Sandbox::new(config);

        // printenv will list all env vars the child process sees.
        let result = sandbox
            .execute("printenv", &["AEGIS_TEST_SECRET_009"])
            .expect("execute printenv");

        // The variable should NOT be visible to the child.
        assert!(
            result.stdout.trim().is_empty(),
            "AEGIS_TEST_SECRET_009 should not be passed to child, \
             got: {}",
            result.stdout
        );
        assert_ne!(result.exit_code, 0, "printenv should fail for missing var");

        unsafe {
            std::env::remove_var("AEGIS_TEST_SECRET_009");
        }
    }

    // @req REQ-SECURITY-009
    #[cfg(unix)]
    #[test]
    fn execute_with_env_allowlist_passes_only_listed_vars() {
        let dir = std::env::temp_dir();
        unsafe {
            std::env::set_var("AEGIS_ALLOWED_VAR", "visible");
            std::env::set_var("AEGIS_BLOCKED_VAR", "hidden");
        }

        let config = SandboxConfig {
            inherit_env: false,
            env_allowlist: vec!["PATH".to_string(), "AEGIS_ALLOWED_VAR".to_string()],
            working_dir: dir,
            ..SandboxConfig::default()
        };
        let sandbox = Sandbox::new(config);

        // Check that the allowed var IS visible.
        let allowed = sandbox
            .execute("printenv", &["AEGIS_ALLOWED_VAR"])
            .expect("execute printenv allowed");
        assert_eq!(allowed.stdout.trim(), "visible");
        assert_eq!(allowed.exit_code, 0);

        // Check that the blocked var is NOT visible.
        let blocked = sandbox
            .execute("printenv", &["AEGIS_BLOCKED_VAR"])
            .expect("execute printenv blocked");
        assert!(
            blocked.stdout.trim().is_empty(),
            "AEGIS_BLOCKED_VAR should not be passed"
        );

        unsafe {
            std::env::remove_var("AEGIS_ALLOWED_VAR");
            std::env::remove_var("AEGIS_BLOCKED_VAR");
        }
    }

    // @req REQ-SECURITY-009
    #[cfg(windows)]
    #[test]
    fn execute_with_inherit_env_false_strips_env_vars_windows() {
        let dir = std::env::temp_dir();
        unsafe {
            std::env::set_var("AEGIS_TEST_SECRET_009", "leaked");
        }

        let config = SandboxConfig {
            inherit_env: false,
            env_allowlist: vec!["PATH".to_string()],
            working_dir: dir,
            ..SandboxConfig::default()
        };
        let sandbox = Sandbox::new(config);

        let result = sandbox
            .execute("cmd", &["/C", "echo", "%AEGIS_TEST_SECRET_009%"])
            .expect("execute cmd");

        // On Windows, unexpanded %VAR% means the var is not set.
        assert!(
            !result.stdout.contains("leaked"),
            "AEGIS_TEST_SECRET_009 should not be passed to child"
        );

        unsafe {
            std::env::remove_var("AEGIS_TEST_SECRET_009");
        }
    }
}
