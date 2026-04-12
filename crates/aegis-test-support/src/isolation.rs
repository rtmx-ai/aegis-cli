//! Tempdir-based test isolation for HOME, sessions, and audit ledger.
//!
//! Each test gets a fresh `~/.aegis` tree without polluting real state.
//!
//! Setting the `HOME` env var is process-global, so a global `Mutex` is used
//! to serialize tests that construct an [`IsolatedHome`]. Tests can still
//! run in parallel; only the critical section that mutates `HOME` is
//! serialized via the guard held by each `IsolatedHome` instance.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use tempfile::TempDir;

/// Global lock serializing access to the `HOME` env var across tests.
fn home_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Owns a tempdir that masquerades as `$HOME` for the duration of a test.
///
/// On construction:
/// - Creates a fresh tempdir.
/// - Acquires the global HOME mutex.
/// - Saves the previous `HOME` (and `USERPROFILE` on Windows).
/// - Sets `HOME` (and `USERPROFILE`) to the tempdir path.
/// - Pre-creates `<tempdir>/.aegis/sessions` and `<tempdir>/.aegis/logs`.
///
/// On drop:
/// - Restores the previous `HOME` (and `USERPROFILE`).
/// - Releases the global HOME mutex.
/// - The tempdir is removed.
pub struct IsolatedHome {
    tempdir: Option<TempDir>,
    original_home: Option<String>,
    #[cfg(windows)]
    original_userprofile: Option<String>,
    _guard: MutexGuard<'static, ()>,
}

impl IsolatedHome {
    /// Create a new isolated HOME with a pre-populated `.aegis` tree.
    pub fn new() -> std::io::Result<Self> {
        let guard = home_lock().lock().unwrap_or_else(|e| e.into_inner());

        let tempdir = tempfile::tempdir()?;
        let path = tempdir.path().to_path_buf();

        let original_home = std::env::var("HOME").ok();
        #[cfg(windows)]
        let original_userprofile = std::env::var("USERPROFILE").ok();

        // SAFETY: HOME mutation is serialized via the global mutex.
        unsafe {
            std::env::set_var("HOME", &path);
            #[cfg(windows)]
            std::env::set_var("USERPROFILE", &path);
        }

        let aegis = path.join(".aegis");
        std::fs::create_dir_all(aegis.join("sessions"))?;
        std::fs::create_dir_all(aegis.join("logs"))?;

        Ok(Self {
            tempdir: Some(tempdir),
            original_home,
            #[cfg(windows)]
            original_userprofile,
            _guard: guard,
        })
    }

    /// The tempdir root acting as `$HOME`.
    pub fn path(&self) -> &Path {
        self.tempdir
            .as_ref()
            .expect("tempdir present until drop")
            .path()
    }

    /// `<tempdir>/.aegis`
    pub fn aegis_dir(&self) -> PathBuf {
        self.path().join(".aegis")
    }

    /// `<tempdir>/.aegis/config.yaml`
    pub fn config_path(&self) -> PathBuf {
        self.aegis_dir().join("config.yaml")
    }

    /// `<tempdir>/.aegis/sessions`
    pub fn sessions_dir(&self) -> PathBuf {
        self.aegis_dir().join("sessions")
    }

    /// `<tempdir>/.aegis/logs`
    pub fn audit_log_dir(&self) -> PathBuf {
        self.aegis_dir().join("logs")
    }
}

impl Drop for IsolatedHome {
    fn drop(&mut self) {
        // SAFETY: HOME mutation is serialized via the global mutex held by self.
        unsafe {
            match &self.original_home {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
            #[cfg(windows)]
            match &self.original_userprofile {
                Some(v) => std::env::set_var("USERPROFILE", v),
                None => std::env::remove_var("USERPROFILE"),
            }
        }
        // Drop tempdir before releasing the guard so cleanup happens under lock.
        self.tempdir.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-TEST-023
    #[test]
    fn test_isolation_creates_fresh_home() {
        let home = IsolatedHome::new().expect("create isolated home");
        let aegis = home.aegis_dir();
        assert!(aegis.exists(), "aegis_dir should exist");
        assert!(aegis.is_dir(), "aegis_dir should be a directory");
        assert!(
            aegis.starts_with(home.path()),
            "aegis_dir must live inside the tempdir"
        );
    }

    // rtmx:req REQ-TEST-023
    #[test]
    fn test_isolation_sets_home_env_var() {
        let home = IsolatedHome::new().expect("create isolated home");
        let env_home = std::env::var("HOME").expect("HOME set");
        assert_eq!(PathBuf::from(env_home), home.path());
    }

    // rtmx:req REQ-TEST-023
    #[test]
    fn test_isolation_restores_home_on_drop() {
        let isolated_path;
        {
            let home = IsolatedHome::new().expect("create isolated home");
            isolated_path = home.path().to_path_buf();
            // HOME is now the tempdir.
            assert_eq!(
                std::env::var("HOME").ok(),
                Some(isolated_path.display().to_string())
            );
        }
        // After drop, HOME must NOT be the isolated tmpdir anymore.
        // (We don't assert a specific value because parallel tests
        // also mutate the process-global HOME env var.)
        assert_ne!(
            std::env::var("HOME").ok(),
            Some(isolated_path.display().to_string()),
            "HOME should be restored away from the isolated tmpdir"
        );
    }

    // rtmx:req REQ-TEST-023
    #[test]
    fn test_isolation_provides_subdirectories() {
        let home = IsolatedHome::new().expect("create isolated home");
        let sessions = home.sessions_dir();
        let logs = home.audit_log_dir();
        assert!(sessions.is_dir(), "sessions dir exists");
        assert!(logs.is_dir(), "audit log dir exists");

        let session_file = sessions.join("s1.json");
        std::fs::write(&session_file, b"{}").expect("write session file");
        assert!(session_file.exists());

        let log_file = logs.join("audit.jsonl");
        std::fs::write(&log_file, b"{}\n").expect("write log file");
        assert!(log_file.exists());

        // config_path is exposed even though the file isn't pre-created.
        assert_eq!(home.config_path(), home.aegis_dir().join("config.yaml"));
    }
}
