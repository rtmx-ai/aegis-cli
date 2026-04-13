//! Pre-write snapshot capture and undo/rollback functionality.
//!
//! Before the agent writes to a file, `RollbackJournal::capture()` records
//! the file's current state. The `/undo` command restores from the most
//! recent snapshot (LIFO). Snapshots persist across restarts via JSON.
//! rtmx:req REQ-HITL-009
//! rtmx:req REQ-HITL-010

use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

/// A snapshot of a file's state before a write operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSnapshot {
    /// Original file path that was written to.
    pub original_path: PathBuf,
    /// Content of the file before the write (None if file didn't exist).
    pub original_content: Option<String>,
    /// Timestamp of the snapshot.
    pub captured_at: chrono::DateTime<chrono::Utc>,
    /// Session ID for grouping.
    pub session_id: String,
}

/// Manages file snapshots for rollback/undo.
pub struct RollbackJournal {
    snapshots: Vec<FileSnapshot>,
    storage_dir: PathBuf,
}

impl RollbackJournal {
    /// Create a new journal that persists snapshots to `storage_dir`.
    pub fn new(storage_dir: PathBuf) -> Self {
        Self {
            snapshots: Vec::new(),
            storage_dir,
        }
    }

    /// Capture the current state of a file before it is modified.
    /// Call this BEFORE the write_file tool executes.
    pub fn capture(&mut self, path: &Path, session_id: &str) -> io::Result<()> {
        let existed = path.exists();
        let original_content = if existed {
            Some(std::fs::read_to_string(path)?)
        } else {
            None
        };

        self.snapshots.push(FileSnapshot {
            original_path: path.to_path_buf(),
            original_content,
            captured_at: chrono::Utc::now(),
            session_id: session_id.to_string(),
        });

        tracing::info!(
            path = %path.display(),
            session_id = session_id,
            existed = existed,
            "Captured pre-write snapshot (REQ-HITL-009)"
        );

        Ok(())
    }

    /// Undo the last write operation by restoring the snapshot.
    /// Returns the path that was restored.
    pub fn undo_last(&mut self) -> io::Result<Option<PathBuf>> {
        let snapshot = match self.snapshots.pop() {
            Some(s) => s,
            None => return Ok(None),
        };

        restore_snapshot(&snapshot)?;
        Ok(Some(snapshot.original_path))
    }

    /// Undo all writes in the current session.
    pub fn undo_all(&mut self, session_id: &str) -> io::Result<Vec<PathBuf>> {
        let mut restored = Vec::new();
        // Drain matching snapshots in reverse order (LIFO).
        let mut remaining = Vec::new();
        // Process all snapshots: restore those matching the session,
        // keep the rest.
        let all = std::mem::take(&mut self.snapshots);
        for snapshot in all.into_iter().rev() {
            if snapshot.session_id == session_id {
                restore_snapshot(&snapshot)?;
                restored.push(snapshot.original_path);
            } else {
                remaining.push(snapshot);
            }
        }
        remaining.reverse();
        self.snapshots = remaining;
        Ok(restored)
    }

    /// List all snapshots (for /undo --list).
    pub fn list(&self) -> &[FileSnapshot] {
        &self.snapshots
    }

    /// Check if the file has been modified since the snapshot was taken.
    /// Returns true if file content differs from snapshot.
    pub fn has_conflict(&self, snapshot: &FileSnapshot) -> io::Result<bool> {
        match &snapshot.original_content {
            Some(original) => {
                if snapshot.original_path.exists() {
                    let current = std::fs::read_to_string(&snapshot.original_path)?;
                    Ok(current != *original)
                } else {
                    // File existed at snapshot time but is now gone.
                    Ok(true)
                }
            }
            None => {
                // File did not exist at snapshot time.
                // Conflict if it now exists (someone else recreated it).
                Ok(snapshot.original_path.exists())
            }
        }
    }

    /// Persist snapshots to disk (JSON file in storage_dir).
    pub fn save(&self) -> io::Result<()> {
        std::fs::create_dir_all(&self.storage_dir)?;
        let path = self.storage_dir.join("snapshots.json");
        let json = serde_json::to_string_pretty(&self.snapshots).map_err(io::Error::other)?;
        std::fs::write(path, json)
    }

    /// Load snapshots from disk.
    pub fn load(storage_dir: &Path) -> io::Result<Self> {
        let path = storage_dir.join("snapshots.json");
        if !path.exists() {
            return Ok(Self::new(storage_dir.to_path_buf()));
        }
        let json = std::fs::read_to_string(&path)?;
        let snapshots: Vec<FileSnapshot> = serde_json::from_str(&json)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(Self {
            snapshots,
            storage_dir: storage_dir.to_path_buf(),
        })
    }
}

/// Restore a single snapshot -- either write original content back or
/// delete the file if it did not previously exist.
fn restore_snapshot(snapshot: &FileSnapshot) -> io::Result<()> {
    match &snapshot.original_content {
        Some(content) => {
            if let Some(parent) = snapshot.original_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&snapshot.original_path, content)?;
            tracing::info!(
                path = %snapshot.original_path.display(),
                "Restored file from snapshot (REQ-HITL-010)"
            );
        }
        None => {
            // File didn't exist before -- remove it.
            if snapshot.original_path.exists() {
                std::fs::remove_file(&snapshot.original_path)?;
                tracing::info!(
                    path = %snapshot.original_path.display(),
                    "Removed file that did not exist before write (REQ-HITL-010)"
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-HITL-009
    #[test]
    fn capture_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("existing.txt");
        std::fs::write(&file, "original content").unwrap();

        let storage = dir.path().join("rollback");
        let mut journal = RollbackJournal::new(storage);
        journal.capture(&file, "sess-1").unwrap();

        assert_eq!(journal.list().len(), 1);
        assert_eq!(
            journal.list()[0].original_content.as_deref(),
            Some("original content")
        );
    }

    // rtmx:req REQ-HITL-009
    #[test]
    fn capture_nonexistent_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("new_file.txt");

        let storage = dir.path().join("rollback");
        let mut journal = RollbackJournal::new(storage);
        journal.capture(&file, "sess-1").unwrap();

        assert_eq!(journal.list().len(), 1);
        assert!(journal.list()[0].original_content.is_none());
    }

    // rtmx:req REQ-HITL-009
    #[test]
    fn capture_stores_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("meta.txt");
        std::fs::write(&file, "data").unwrap();

        let storage = dir.path().join("rollback");
        let mut journal = RollbackJournal::new(storage);
        journal.capture(&file, "session-42").unwrap();

        let snap = &journal.list()[0];
        assert_eq!(snap.session_id, "session-42");
        assert!(snap.captured_at <= chrono::Utc::now());
        assert_eq!(snap.original_path, file);
    }

    // rtmx:req REQ-HITL-009
    #[test]
    fn multiple_captures_stacked() {
        let dir = tempfile::tempdir().unwrap();
        let file_a = dir.path().join("a.txt");
        let file_b = dir.path().join("b.txt");
        std::fs::write(&file_a, "aaa").unwrap();
        std::fs::write(&file_b, "bbb").unwrap();

        let storage = dir.path().join("rollback");
        let mut journal = RollbackJournal::new(storage);
        journal.capture(&file_a, "s1").unwrap();
        journal.capture(&file_b, "s1").unwrap();

        assert_eq!(journal.list().len(), 2);
        // Last captured is last in the vec (LIFO pop).
        assert_eq!(journal.list()[1].original_path, file_b);
    }

    // rtmx:req REQ-HITL-010
    #[test]
    fn undo_last_restores_content() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("restore.txt");
        std::fs::write(&file, "before").unwrap();

        let storage = dir.path().join("rollback");
        let mut journal = RollbackJournal::new(storage);
        journal.capture(&file, "s1").unwrap();

        // Simulate the write tool overwriting the file.
        std::fs::write(&file, "after").unwrap();

        let restored = journal.undo_last().unwrap();
        assert_eq!(restored, Some(file.clone()));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "before");
    }

    // rtmx:req REQ-HITL-010
    #[test]
    fn undo_last_deletes_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("brand_new.txt");

        let storage = dir.path().join("rollback");
        let mut journal = RollbackJournal::new(storage);
        journal.capture(&file, "s1").unwrap();

        // Simulate write tool creating the file.
        std::fs::write(&file, "created").unwrap();
        assert!(file.exists());

        journal.undo_last().unwrap();
        assert!(!file.exists());
    }

    // rtmx:req REQ-HITL-010
    #[test]
    fn undo_all_restores_session() {
        let dir = tempfile::tempdir().unwrap();
        let file_a = dir.path().join("a.txt");
        let file_b = dir.path().join("b.txt");
        std::fs::write(&file_a, "orig-a").unwrap();
        std::fs::write(&file_b, "orig-b").unwrap();

        let storage = dir.path().join("rollback");
        let mut journal = RollbackJournal::new(storage);
        journal.capture(&file_a, "target-session").unwrap();
        journal.capture(&file_b, "target-session").unwrap();

        // Simulate writes.
        std::fs::write(&file_a, "modified-a").unwrap();
        std::fs::write(&file_b, "modified-b").unwrap();

        let restored = journal.undo_all("target-session").unwrap();
        assert_eq!(restored.len(), 2);
        assert_eq!(std::fs::read_to_string(&file_a).unwrap(), "orig-a");
        assert_eq!(std::fs::read_to_string(&file_b).unwrap(), "orig-b");
    }

    // rtmx:req REQ-HITL-010
    #[test]
    fn undo_empty_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().join("rollback");
        let mut journal = RollbackJournal::new(storage);

        assert_eq!(journal.undo_last().unwrap(), None);
    }

    // rtmx:req REQ-HITL-010
    #[test]
    fn has_conflict_detects_change() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("conflict.txt");
        std::fs::write(&file, "original").unwrap();

        let storage = dir.path().join("rollback");
        let mut journal = RollbackJournal::new(storage);
        journal.capture(&file, "s1").unwrap();

        // Modify file after snapshot.
        std::fs::write(&file, "changed by someone else").unwrap();

        assert!(journal.has_conflict(&journal.list()[0].clone()).unwrap());
    }

    // rtmx:req REQ-HITL-010
    #[test]
    fn has_conflict_no_change() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("stable.txt");
        std::fs::write(&file, "same").unwrap();

        let storage = dir.path().join("rollback");
        let mut journal = RollbackJournal::new(storage);
        journal.capture(&file, "s1").unwrap();

        assert!(!journal.has_conflict(&journal.list()[0].clone()).unwrap());
    }

    // rtmx:req REQ-HITL-010
    #[test]
    fn journal_save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("roundtrip.txt");
        std::fs::write(&file, "persisted").unwrap();

        let storage = dir.path().join("rollback");
        let mut journal = RollbackJournal::new(storage.clone());
        journal.capture(&file, "sess-rt").unwrap();
        journal.save().unwrap();

        let loaded = RollbackJournal::load(&storage).unwrap();
        assert_eq!(loaded.list().len(), 1);
        assert_eq!(loaded.list()[0].session_id, "sess-rt");
        assert_eq!(
            loaded.list()[0].original_content.as_deref(),
            Some("persisted")
        );
        assert_eq!(loaded.list()[0].original_path, file);
    }
}
