//! Session persistence: serialize agent state to disk so interrupted sessions
//! can be resumed.
//!
//! Layered as: snapshot type (REQ-AGENT-030) -> save (REQ-AGENT-031) ->
//! load (REQ-AGENT-032). The parent REQ-AGENT-028 is satisfied by the
//! roundtrip integration test.

use aegis_domain::ports::Message;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Schema version for on-disk session snapshots. Bump on breaking changes.
pub const SESSION_SCHEMA_VERSION: u32 = 1;

/// Serializable snapshot of an in-flight agent session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionSnapshot {
    pub schema_version: u32,
    pub session_id: String,
    pub messages: Vec<Message>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub model_name: String,
    pub working_dir: PathBuf,
    /// Unix epoch seconds when the snapshot was taken.
    pub timestamp: u64,
}

impl SessionSnapshot {
    /// Build a new snapshot, stamping `timestamp` to the current wall clock
    /// and `schema_version` to the current constant.
    pub fn new(
        session_id: impl Into<String>,
        messages: Vec<Message>,
        input_tokens: u64,
        output_tokens: u64,
        model_name: impl Into<String>,
        working_dir: PathBuf,
    ) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            schema_version: SESSION_SCHEMA_VERSION,
            session_id: session_id.into(),
            messages,
            input_tokens,
            output_tokens,
            model_name: model_name.into(),
            working_dir,
            timestamp,
        }
    }
}

/// Default session directory: `~/.aegis/sessions`.
///
/// Returns `None` if the home directory cannot be determined.
pub fn default_session_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".aegis").join("sessions"))
}

/// Save a session snapshot atomically to `<dir>/<session_id>.json`.
///
/// Writes to a sibling tmp file then renames into place to avoid torn writes.
/// Creates the directory tree (mode 0700 on unix) if needed. Also updates a
/// `current.json` symlink (unix) pointing at the latest snapshot.
pub fn save_session(dir: &Path, snapshot: &SessionSnapshot) -> io::Result<PathBuf> {
    create_session_dir(dir)?;

    let final_path = dir.join(format!("{}.json", snapshot.session_id));
    let tmp_path = dir.join(format!(".{}.json.tmp", snapshot.session_id));

    let json = serde_json::to_vec_pretty(snapshot)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    std::fs::write(&tmp_path, &json)?;
    std::fs::rename(&tmp_path, &final_path)?;

    update_current_symlink(dir, &final_path)?;

    Ok(final_path)
}

#[cfg(unix)]
fn create_session_dir(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(dir)?;
    let perms = std::fs::Permissions::from_mode(0o700);
    std::fs::set_permissions(dir, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn create_session_dir(dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir)
}

#[cfg(unix)]
fn update_current_symlink(dir: &Path, target: &Path) -> io::Result<()> {
    let link = dir.join("current.json");
    if link.exists() || link.symlink_metadata().is_ok() {
        let _ = std::fs::remove_file(&link);
    }
    // Use just the filename so the symlink stays valid if the dir moves.
    let file_name = target.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "snapshot path has no file name",
        )
    })?;
    std::os::unix::fs::symlink(file_name, &link)
}

#[cfg(not(unix))]
fn update_current_symlink(_dir: &Path, _target: &Path) -> io::Result<()> {
    Ok(())
}

/// Load a session snapshot from disk.
///
/// Returns `None` if the file is missing or cannot be parsed (corrupt,
/// truncated, or schema mismatch). Never panics — callers should treat
/// `None` as "start a fresh session".
pub fn load_session(path: &Path) -> Option<SessionSnapshot> {
    let bytes = std::fs::read(path).ok()?;
    let snapshot: SessionSnapshot = serde_json::from_slice(&bytes).ok()?;
    if snapshot.schema_version != SESSION_SCHEMA_VERSION {
        return None;
    }
    Some(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_domain::ports::Role;
    use tempfile::tempdir;

    fn sample_snapshot() -> SessionSnapshot {
        SessionSnapshot::new(
            "sess-abc",
            vec![
                Message {
                    role: Role::User,
                    content: "hello".into(),
                },
                Message {
                    role: Role::Assistant,
                    content: "world".into(),
                },
            ],
            42,
            17,
            "claude-opus-4-6",
            PathBuf::from("/tmp/work"),
        )
    }

    /// rtmx:req REQ-AGENT-030
    #[test]
    fn test_session_snapshot_roundtrip() {
        let snap = sample_snapshot();
        let json = serde_json::to_string(&snap).expect("serialize");
        let back: SessionSnapshot = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(snap, back);
        assert_eq!(back.schema_version, SESSION_SCHEMA_VERSION);
        assert_eq!(back.messages.len(), 2);
        assert_eq!(back.input_tokens, 42);
        assert_eq!(back.output_tokens, 17);
        assert_eq!(back.model_name, "claude-opus-4-6");
    }

    /// rtmx:req REQ-AGENT-031
    #[test]
    fn test_session_save_creates_file() {
        let dir = tempdir().expect("tempdir");
        let snap = sample_snapshot();
        let path = save_session(dir.path(), &snap).expect("save");
        assert!(path.exists(), "snapshot file should exist after save");
        assert_eq!(path.file_name().unwrap(), "sess-abc.json");

        // Atomic write should not leave a tmp file behind.
        let tmp = dir.path().join(".sess-abc.json.tmp");
        assert!(!tmp.exists(), "tmp file should be renamed away");
    }

    /// rtmx:req REQ-AGENT-031
    #[cfg(unix)]
    #[test]
    fn test_session_save_sets_dir_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempdir().expect("tempdir");
        let nested = dir.path().join("sessions");
        let snap = sample_snapshot();
        save_session(&nested, &snap).expect("save");
        let mode = std::fs::metadata(&nested).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "session dir must be 0700");
    }

    /// rtmx:req REQ-AGENT-031
    #[cfg(unix)]
    #[test]
    fn test_session_save_updates_current_symlink() {
        let dir = tempdir().expect("tempdir");
        let snap = sample_snapshot();
        save_session(dir.path(), &snap).expect("save");
        let link = dir.path().join("current.json");
        assert!(link.symlink_metadata().is_ok(), "current.json should exist");
        let target = std::fs::read_link(&link).expect("read_link");
        assert_eq!(target, PathBuf::from("sess-abc.json"));
    }

    /// rtmx:req REQ-AGENT-032
    #[test]
    fn test_session_load_restores_state() {
        let dir = tempdir().expect("tempdir");
        let snap = sample_snapshot();
        let path = save_session(dir.path(), &snap).expect("save");
        let loaded = load_session(&path).expect("load");
        assert_eq!(loaded, snap);
    }

    /// rtmx:req REQ-AGENT-032
    #[test]
    fn test_session_load_returns_none_for_missing_file() {
        let dir = tempdir().expect("tempdir");
        let missing = dir.path().join("nope.json");
        assert!(load_session(&missing).is_none());
    }

    /// rtmx:req REQ-AGENT-032
    #[test]
    fn test_session_load_returns_none_for_corrupt_file() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("bad.json");
        std::fs::write(&path, b"{not valid json").unwrap();
        assert!(load_session(&path).is_none());
    }

    /// rtmx:req REQ-AGENT-032
    #[test]
    fn test_session_load_rejects_unknown_schema_version() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("future.json");
        let json = r#"{"schema_version":9999,"session_id":"x","messages":[],"input_tokens":0,"output_tokens":0,"model_name":"m","working_dir":"/tmp","timestamp":0}"#;
        std::fs::write(&path, json).unwrap();
        assert!(load_session(&path).is_none());
    }

    /// rtmx:req REQ-AGENT-028
    /// Parent requirement: end-to-end save then load roundtrip on disk.
    #[test]
    fn test_session_save_and_load() {
        let dir = tempdir().expect("tempdir");
        let snap = sample_snapshot();
        let path = save_session(dir.path(), &snap).expect("save");
        let loaded = load_session(&path).expect("load");
        assert_eq!(loaded.session_id, "sess-abc");
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.input_tokens, 42);
        assert_eq!(loaded.output_tokens, 17);
    }
}
