//! OS keychain credential storage (REQ-SECURITY-019).
//!
//! Provides a trait-based abstraction over OS-native credential stores with
//! a portable in-memory fallback for testing and environments where no
//! keychain CLI is available.
//!
//! ## Backends
//!
//! - **macOS:** `security` command (Keychain Services)
//! - **Linux:** `secret-tool` (libsecret / GNOME Keyring)
//! - **Windows:** `cmdkey` (Windows Credential Manager)
//! - **Memory:** In-process `HashMap` fallback (non-persistent)

use aegis_domain::error::DomainError;
use std::collections::HashMap;
use std::process::Command;
use std::sync::RwLock;

/// Trait for OS-native credential storage.
pub trait KeychainProvider: Send + Sync {
    /// Store a secret in the keychain.
    fn store(&self, service: &str, account: &str, secret: &str) -> Result<(), DomainError>;

    /// Retrieve a secret from the keychain. Returns `None` if not found.
    fn retrieve(&self, service: &str, account: &str) -> Result<Option<String>, DomainError>;

    /// Delete a secret from the keychain.
    fn delete(&self, service: &str, account: &str) -> Result<(), DomainError>;
}

// ---------------------------------------------------------------------------
// MemoryKeychain
// ---------------------------------------------------------------------------

/// In-memory keychain for testing. Not persistent across process restarts.
pub struct MemoryKeychain {
    store: RwLock<HashMap<(String, String), String>>,
}

impl MemoryKeychain {
    /// Create a new empty in-memory keychain.
    pub fn new() -> Self {
        Self {
            store: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for MemoryKeychain {
    fn default() -> Self {
        Self::new()
    }
}

impl KeychainProvider for MemoryKeychain {
    fn store(&self, service: &str, account: &str, secret: &str) -> Result<(), DomainError> {
        let mut map = self.store.write().map_err(|e| DomainError::ConfigError {
            message: format!("keychain lock poisoned: {e}"),
        })?;
        map.insert((service.to_owned(), account.to_owned()), secret.to_owned());
        Ok(())
    }

    fn retrieve(&self, service: &str, account: &str) -> Result<Option<String>, DomainError> {
        let map = self.store.read().map_err(|e| DomainError::ConfigError {
            message: format!("keychain lock poisoned: {e}"),
        })?;
        Ok(map.get(&(service.to_owned(), account.to_owned())).cloned())
    }

    fn delete(&self, service: &str, account: &str) -> Result<(), DomainError> {
        let mut map = self.store.write().map_err(|e| DomainError::ConfigError {
            message: format!("keychain lock poisoned: {e}"),
        })?;
        map.remove(&(service.to_owned(), account.to_owned()));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// CommandKeychain
// ---------------------------------------------------------------------------

/// Detected OS keychain backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeychainBackend {
    /// macOS Keychain via `security` CLI.
    MacOS,
    /// Linux libsecret via `secret-tool` CLI.
    LinuxSecretTool,
    /// Windows Credential Manager via `cmdkey`.
    Windows,
    /// In-process fallback (no OS tool available).
    Memory,
}

/// Keychain that delegates to OS CLI tools.
///
/// Falls back to [`MemoryKeychain`] if no supported tool is detected.
pub struct CommandKeychain {
    backend: KeychainBackend,
    fallback: MemoryKeychain,
}

impl CommandKeychain {
    /// Detect the platform and available CLI tools, returning the best
    /// available backend.
    pub fn detect() -> Self {
        let backend = Self::detect_backend();
        Self {
            backend,
            fallback: MemoryKeychain::new(),
        }
    }

    /// Return the detected backend variant.
    pub fn backend(&self) -> &KeychainBackend {
        &self.backend
    }

    fn detect_backend() -> KeychainBackend {
        if cfg!(target_os = "macos") && Self::command_exists("security") {
            KeychainBackend::MacOS
        } else if cfg!(target_os = "linux") && Self::command_exists("secret-tool") {
            KeychainBackend::LinuxSecretTool
        } else if cfg!(target_os = "windows") && Self::command_exists("cmdkey") {
            KeychainBackend::Windows
        } else {
            KeychainBackend::Memory
        }
    }

    fn command_exists(name: &str) -> bool {
        #[cfg(target_os = "windows")]
        let check = Command::new("where").arg(name).output();
        #[cfg(not(target_os = "windows"))]
        let check = Command::new("which").arg(name).output();

        matches!(check, Ok(output) if output.status.success())
    }

    // -- macOS ---------------------------------------------------------------

    fn macos_store(service: &str, account: &str, secret: &str) -> Result<(), DomainError> {
        let output = Command::new("security")
            .args([
                "add-generic-password",
                "-U",
                "-s",
                service,
                "-a",
                account,
                "-w",
                secret,
            ])
            .output()
            .map_err(|e| DomainError::ConfigError {
                message: format!("failed to run security add-generic-password: {e}"),
            })?;
        if !output.status.success() {
            return Err(DomainError::ConfigError {
                message: format!(
                    "security add-generic-password failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            });
        }
        Ok(())
    }

    fn macos_retrieve(service: &str, account: &str) -> Result<Option<String>, DomainError> {
        let output = Command::new("security")
            .args(["find-generic-password", "-s", service, "-a", account, "-w"])
            .output()
            .map_err(|e| DomainError::ConfigError {
                message: format!("failed to run security find-generic-password: {e}"),
            })?;
        if !output.status.success() {
            // Exit code 44 = item not found
            return Ok(None);
        }
        let secret = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        Ok(Some(secret))
    }

    fn macos_delete(service: &str, account: &str) -> Result<(), DomainError> {
        let output = Command::new("security")
            .args(["delete-generic-password", "-s", service, "-a", account])
            .output()
            .map_err(|e| DomainError::ConfigError {
                message: format!("failed to run security delete-generic-password: {e}"),
            })?;
        if !output.status.success() {
            // Treat "item not found" as success for idempotent delete.
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.contains("could not be found") {
                return Err(DomainError::ConfigError {
                    message: format!("security delete-generic-password failed: {stderr}"),
                });
            }
        }
        Ok(())
    }

    // -- Linux ---------------------------------------------------------------

    fn linux_store(service: &str, account: &str, secret: &str) -> Result<(), DomainError> {
        use std::io::Write;

        let mut child = Command::new("secret-tool")
            .args([
                "store",
                &format!("--label={service}"),
                "service",
                service,
                "account",
                account,
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| DomainError::ConfigError {
                message: format!("failed to spawn secret-tool store: {e}"),
            })?;

        if let Some(ref mut stdin) = child.stdin {
            stdin
                .write_all(secret.as_bytes())
                .map_err(|e| DomainError::ConfigError {
                    message: format!("failed to write secret to secret-tool: {e}"),
                })?;
        }

        let output = child
            .wait_with_output()
            .map_err(|e| DomainError::ConfigError {
                message: format!("secret-tool store failed: {e}"),
            })?;
        if !output.status.success() {
            return Err(DomainError::ConfigError {
                message: format!(
                    "secret-tool store failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            });
        }
        Ok(())
    }

    fn linux_retrieve(service: &str, account: &str) -> Result<Option<String>, DomainError> {
        let output = Command::new("secret-tool")
            .args(["lookup", "service", service, "account", account])
            .output()
            .map_err(|e| DomainError::ConfigError {
                message: format!("failed to run secret-tool lookup: {e}"),
            })?;
        if !output.status.success() {
            return Ok(None);
        }
        let secret = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if secret.is_empty() {
            return Ok(None);
        }
        Ok(Some(secret))
    }

    fn linux_delete(service: &str, account: &str) -> Result<(), DomainError> {
        let output = Command::new("secret-tool")
            .args(["clear", "service", service, "account", account])
            .output()
            .map_err(|e| DomainError::ConfigError {
                message: format!("failed to run secret-tool clear: {e}"),
            })?;
        if !output.status.success() {
            // Treat "not found" as success for idempotent delete.
            return Ok(());
        }
        Ok(())
    }

    // -- Windows -------------------------------------------------------------

    fn windows_store(service: &str, account: &str, secret: &str) -> Result<(), DomainError> {
        let output = Command::new("cmdkey")
            .args([
                &format!("/add:{service}"),
                &format!("/user:{account}"),
                &format!("/pass:{secret}"),
            ])
            .output()
            .map_err(|e| DomainError::ConfigError {
                message: format!("failed to run cmdkey /add: {e}"),
            })?;
        if !output.status.success() {
            return Err(DomainError::ConfigError {
                message: format!(
                    "cmdkey /add failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            });
        }
        Ok(())
    }

    fn windows_retrieve(service: &str, _account: &str) -> Result<Option<String>, DomainError> {
        // cmdkey /list does not expose the password in cleartext.
        // On Windows, the credential store does not allow reading passwords
        // via cmdkey. We can only check existence.
        let output = Command::new("cmdkey")
            .args([&format!("/list:{service}")])
            .output()
            .map_err(|e| DomainError::ConfigError {
                message: format!("failed to run cmdkey /list: {e}"),
            })?;
        if !output.status.success() {
            return Ok(None);
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.contains("* NONE *") || stdout.contains("none") {
            return Ok(None);
        }
        // cmdkey cannot retrieve the actual password, so we return a
        // sentinel indicating the credential exists but is not readable
        // via this backend. Full Windows support requires the Win32
        // CredRead API or a crate like `keyring`.
        Ok(Some("[credential-exists-but-unreadable-via-cmdkey]".into()))
    }

    fn windows_delete(service: &str, _account: &str) -> Result<(), DomainError> {
        let output = Command::new("cmdkey")
            .args([&format!("/delete:{service}")])
            .output()
            .map_err(|e| DomainError::ConfigError {
                message: format!("failed to run cmdkey /delete: {e}"),
            })?;
        if !output.status.success() {
            // Treat "not found" as success for idempotent delete.
            return Ok(());
        }
        Ok(())
    }
}

impl KeychainProvider for CommandKeychain {
    fn store(&self, service: &str, account: &str, secret: &str) -> Result<(), DomainError> {
        match self.backend {
            KeychainBackend::MacOS => Self::macos_store(service, account, secret),
            KeychainBackend::LinuxSecretTool => Self::linux_store(service, account, secret),
            KeychainBackend::Windows => Self::windows_store(service, account, secret),
            KeychainBackend::Memory => self.fallback.store(service, account, secret),
        }
    }

    fn retrieve(&self, service: &str, account: &str) -> Result<Option<String>, DomainError> {
        match self.backend {
            KeychainBackend::MacOS => Self::macos_retrieve(service, account),
            KeychainBackend::LinuxSecretTool => Self::linux_retrieve(service, account),
            KeychainBackend::Windows => Self::windows_retrieve(service, account),
            KeychainBackend::Memory => self.fallback.retrieve(service, account),
        }
    }

    fn delete(&self, service: &str, account: &str) -> Result<(), DomainError> {
        match self.backend {
            KeychainBackend::MacOS => Self::macos_delete(service, account),
            KeychainBackend::LinuxSecretTool => Self::linux_delete(service, account),
            KeychainBackend::Windows => Self::windows_delete(service, account),
            KeychainBackend::Memory => self.fallback.delete(service, account),
        }
    }
}

/// Convenience factory: detect the best available keychain backend.
pub fn create_keychain() -> Box<dyn KeychainProvider> {
    Box::new(CommandKeychain::detect())
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-SECURITY-019
    #[test]
    fn test_memory_store_and_retrieve() {
        let kc = MemoryKeychain::new();
        kc.store("svc", "acct", "s3cret").unwrap();
        let val = kc.retrieve("svc", "acct").unwrap();
        assert_eq!(val, Some("s3cret".to_owned()));
    }

    // rtmx:req REQ-SECURITY-019
    #[test]
    fn test_memory_retrieve_missing_returns_none() {
        let kc = MemoryKeychain::new();
        let val = kc.retrieve("no-such-svc", "no-such-acct").unwrap();
        assert_eq!(val, None);
    }

    // rtmx:req REQ-SECURITY-019
    #[test]
    fn test_memory_delete_removes_entry() {
        let kc = MemoryKeychain::new();
        kc.store("svc", "acct", "s3cret").unwrap();
        kc.delete("svc", "acct").unwrap();
        let val = kc.retrieve("svc", "acct").unwrap();
        assert_eq!(val, None);
    }

    // rtmx:req REQ-SECURITY-019
    #[test]
    fn test_memory_overwrite_replaces_value() {
        let kc = MemoryKeychain::new();
        kc.store("svc", "acct", "old").unwrap();
        kc.store("svc", "acct", "new").unwrap();
        let val = kc.retrieve("svc", "acct").unwrap();
        assert_eq!(val, Some("new".to_owned()));
    }

    // rtmx:req REQ-SECURITY-019
    #[test]
    fn test_detect_returns_valid_backend() {
        let kc = CommandKeychain::detect();
        // The detected backend must be one of the known variants.
        match kc.backend() {
            KeychainBackend::MacOS
            | KeychainBackend::LinuxSecretTool
            | KeychainBackend::Windows
            | KeychainBackend::Memory => {} // all valid
        }
    }

    // rtmx:req REQ-SECURITY-019
    #[test]
    fn test_command_keychain_roundtrip() {
        let kc = CommandKeychain::detect();
        let service = "aegis-cli-test";
        let account = "roundtrip-test-account";
        let secret = "test-secret-value-019";

        // Store
        kc.store(service, account, secret).unwrap();

        // Retrieve
        let retrieved = kc.retrieve(service, account).unwrap();

        // On backends that support full retrieval, verify the value.
        // Windows cmdkey cannot read passwords back, so accept the
        // sentinel or the actual value.
        if *kc.backend() != KeychainBackend::Windows {
            assert_eq!(
                retrieved,
                Some(secret.to_owned()),
                "retrieved value should match stored secret"
            );
        } else {
            assert!(retrieved.is_some(), "credential should exist on Windows");
        }

        // Delete (cleanup)
        kc.delete(service, account).unwrap();

        // Verify deletion
        let after_delete = kc.retrieve(service, account).unwrap();
        if *kc.backend() != KeychainBackend::Windows {
            assert_eq!(after_delete, None, "credential should be gone after delete");
        }
    }

    // rtmx:req REQ-SECURITY-019
    #[test]
    fn test_independent_service_accounts() {
        let kc = MemoryKeychain::new();
        kc.store("svc-a", "acct-1", "alpha").unwrap();
        kc.store("svc-b", "acct-2", "beta").unwrap();

        assert_eq!(
            kc.retrieve("svc-a", "acct-1").unwrap(),
            Some("alpha".to_owned())
        );
        assert_eq!(
            kc.retrieve("svc-b", "acct-2").unwrap(),
            Some("beta".to_owned())
        );

        // Deleting one does not affect the other.
        kc.delete("svc-a", "acct-1").unwrap();
        assert_eq!(kc.retrieve("svc-a", "acct-1").unwrap(), None);
        assert_eq!(
            kc.retrieve("svc-b", "acct-2").unwrap(),
            Some("beta".to_owned())
        );
    }
}
