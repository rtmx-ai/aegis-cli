//! Rollback journal for approved write operations.
//!
//! Before an approved write_file or run_command tool call executes,
//! `RollbackJournal::snapshot()` captures the current state of all files
//! that will be modified. This enables `/undo` to restore previous state.
//!
//! The journal is bounded (default 50 entries) and lives in memory only,
//! resetting each session.
//!
//! rtmx:req REQ-HITL-005

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

/// A single rollback entry capturing pre-write state.
#[derive(Debug, Clone)]
pub struct RollbackEntry {
    /// Unique ID for this entry (monotonic counter).
    pub id: u64,
    /// The tool call that triggered this snapshot.
    pub tool_description: String,
    /// Timestamp when the snapshot was taken (ISO 8601).
    pub timestamp: String,
    /// File snapshots captured before the write.
    pub snapshots: Vec<FileSnapshot>,
}

/// Pre-write snapshot of a single file.
#[derive(Debug, Clone)]
pub struct FileSnapshot {
    /// Absolute path to the file.
    pub path: PathBuf,
    /// File contents before the write. None if the file did not exist (new file).
    pub previous_contents: Option<String>,
    /// Whether the file existed before the write.
    pub existed: bool,
}

/// Rolling journal of recent write operations with pre-write snapshots.
pub struct RollbackJournal {
    entries: VecDeque<RollbackEntry>,
    max_entries: usize,
    next_id: u64,
}

/// Errors that can occur during rollback operations.
#[derive(Debug, thiserror::Error)]
pub enum RollbackError {
    /// The requested entry ID was not found in the journal.
    #[error("entry {id} not found in journal")]
    EntryNotFound {
        /// The ID that was requested.
        id: u64,
    },
    /// Failed to read a file when taking a snapshot.
    #[error("failed to read file for snapshot: {path}")]
    SnapshotFailed {
        /// The file path that could not be read.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// Failed to restore a file during rollback.
    #[error("failed to restore file: {path}")]
    RestoreFailed {
        /// The file path that could not be restored.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
}

impl RollbackJournal {
    /// Create a new journal with bounded capacity.
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            max_entries,
            next_id: 1,
        }
    }

    /// Snapshot the current state of files that will be modified.
    /// Call this BEFORE the tool executor writes.
    ///
    /// Returns the entry ID assigned to this snapshot.
    /// File read failures are recorded as best-effort: the snapshot
    /// still proceeds but the file contents will be `None`.
    pub fn snapshot(&mut self, paths: &[&Path], tool_description: &str) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        let snapshots: Vec<FileSnapshot> = paths
            .iter()
            .map(|p| {
                let existed = p.exists();
                // best-effort: record None on read failure
                let previous_contents = if existed {
                    std::fs::read_to_string(p).ok()
                } else {
                    None
                };
                FileSnapshot {
                    path: p.to_path_buf(),
                    previous_contents,
                    existed,
                }
            })
            .collect();

        let entry = RollbackEntry {
            id,
            tool_description: tool_description.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            snapshots,
        };

        // Evict oldest if at capacity.
        if self.entries.len() >= self.max_entries {
            self.entries.pop_front();
        }

        self.entries.push_back(entry);
        id
    }

    /// Roll back a specific entry, restoring files to pre-write state.
    /// Returns the paths that were restored.
    pub fn rollback(&mut self, entry_id: u64) -> Result<Vec<PathBuf>, RollbackError> {
        let pos = self
            .entries
            .iter()
            .position(|e| e.id == entry_id)
            .ok_or(RollbackError::EntryNotFound { id: entry_id })?;

        let entry = self.entries.remove(pos).expect("position was valid");
        restore_entry(&entry)
    }

    /// Roll back the most recent entry.
    pub fn rollback_last(&mut self) -> Result<Vec<PathBuf>, RollbackError> {
        let entry = self
            .entries
            .pop_back()
            .ok_or(RollbackError::EntryNotFound { id: 0 })?;
        restore_entry(&entry)
    }

    /// List recent entries (newest first).
    pub fn recent(&self, count: usize) -> Vec<&RollbackEntry> {
        self.entries.iter().rev().take(count).collect()
    }

    /// Number of entries in the journal.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the journal is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Restore all file snapshots in an entry. Returns the paths restored.
fn restore_entry(entry: &RollbackEntry) -> Result<Vec<PathBuf>, RollbackError> {
    let mut restored = Vec::new();
    for snap in &entry.snapshots {
        if snap.existed {
            if let Some(ref content) = snap.previous_contents {
                if let Some(parent) = snap.path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        RollbackError::RestoreFailed {
                            path: snap.path.clone(),
                            source: e,
                        }
                    })?;
                }
                std::fs::write(&snap.path, content).map_err(|e| {
                    RollbackError::RestoreFailed {
                        path: snap.path.clone(),
                        source: e,
                    }
                })?;
            }
        } else {
            // File did not exist before -- remove it if it now exists.
            if snap.path.exists() {
                std::fs::remove_file(&snap.path).map_err(|e| RollbackError::RestoreFailed {
                    path: snap.path.clone(),
                    source: e,
                })?;
            }
        }
        restored.push(snap.path.clone());
    }
    Ok(restored)
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-HITL-005
    #[test]
    fn test_snapshot_captures_existing_file_contents() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("existing.txt");
        std::fs::write(&file, "hello world").unwrap();

        let mut journal = RollbackJournal::new(50);
        let id = journal.snapshot(&[file.as_path()], "write_file");

        assert_eq!(id, 1);
        assert_eq!(journal.len(), 1);
        let entry = &journal.entries[0];
        assert_eq!(entry.snapshots.len(), 1);
        assert_eq!(
            entry.snapshots[0].previous_contents.as_deref(),
            Some("hello world")
        );
        assert!(entry.snapshots[0].existed);
    }

    // rtmx:req REQ-HITL-005
    #[test]
    fn test_snapshot_records_nonexistent_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("does_not_exist.txt");

        let mut journal = RollbackJournal::new(50);
        journal.snapshot(&[file.as_path()], "write_file");

        let entry = &journal.entries[0];
        assert!(!entry.snapshots[0].existed);
        assert!(entry.snapshots[0].previous_contents.is_none());
    }

    // rtmx:req REQ-HITL-005
    #[test]
    fn test_rollback_restores_original_contents() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("restore.txt");
        std::fs::write(&file, "original").unwrap();

        let mut journal = RollbackJournal::new(50);
        let id = journal.snapshot(&[file.as_path()], "write_file");

        // Simulate the tool overwriting the file.
        std::fs::write(&file, "modified").unwrap();

        let paths = journal.rollback(id).unwrap();
        assert_eq!(paths, vec![file.clone()]);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "original");
    }

    // rtmx:req REQ-HITL-005
    #[test]
    fn test_rollback_deletes_newly_created_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("new_file.txt");

        let mut journal = RollbackJournal::new(50);
        let id = journal.snapshot(&[file.as_path()], "write_file");

        // Simulate the tool creating the file.
        std::fs::write(&file, "brand new").unwrap();
        assert!(file.exists());

        journal.rollback(id).unwrap();
        assert!(!file.exists());
    }

    // rtmx:req REQ-HITL-005
    #[test]
    fn test_rollback_last_rolls_back_most_recent() {
        let dir = tempfile::tempdir().unwrap();
        let file_a = dir.path().join("a.txt");
        let file_b = dir.path().join("b.txt");
        let file_c = dir.path().join("c.txt");
        std::fs::write(&file_a, "orig-a").unwrap();
        std::fs::write(&file_b, "orig-b").unwrap();
        std::fs::write(&file_c, "orig-c").unwrap();

        let mut journal = RollbackJournal::new(50);
        journal.snapshot(&[file_a.as_path()], "write a");
        journal.snapshot(&[file_b.as_path()], "write b");
        journal.snapshot(&[file_c.as_path()], "write c");

        // Overwrite all files.
        std::fs::write(&file_a, "new-a").unwrap();
        std::fs::write(&file_b, "new-b").unwrap();
        std::fs::write(&file_c, "new-c").unwrap();

        // Only the last entry (file_c) should be rolled back.
        let paths = journal.rollback_last().unwrap();
        assert_eq!(paths, vec![file_c.clone()]);
        assert_eq!(std::fs::read_to_string(&file_c).unwrap(), "orig-c");

        // file_a and file_b should still be modified.
        assert_eq!(std::fs::read_to_string(&file_a).unwrap(), "new-a");
        assert_eq!(std::fs::read_to_string(&file_b).unwrap(), "new-b");

        // Journal should have 2 remaining entries.
        assert_eq!(journal.len(), 2);
    }

    // rtmx:req REQ-HITL-005
    #[test]
    fn test_journal_capacity_evicts_oldest() {
        let mut journal = RollbackJournal::new(3);
        let dir = tempfile::tempdir().unwrap();

        // Add 5 entries.
        let mut ids = Vec::new();
        for i in 0..5 {
            let file = dir.path().join(format!("file_{i}.txt"));
            std::fs::write(&file, format!("content-{i}")).unwrap();
            let id = journal.snapshot(&[file.as_path()], &format!("write {i}"));
            ids.push(id);
        }

        // Only 3 should remain.
        assert_eq!(journal.len(), 3);

        // Oldest 2 should be evicted (ids 1 and 2).
        let result = journal.rollback(ids[0]);
        assert!(matches!(result, Err(RollbackError::EntryNotFound { .. })));
        let result = journal.rollback(ids[1]);
        assert!(matches!(result, Err(RollbackError::EntryNotFound { .. })));

        // Newest 3 should still be present (ids 3, 4, 5).
        assert!(journal.entries.iter().any(|e| e.id == ids[2]));
        assert!(journal.entries.iter().any(|e| e.id == ids[3]));
        assert!(journal.entries.iter().any(|e| e.id == ids[4]));
    }

    // rtmx:req REQ-HITL-005
    #[test]
    fn test_rollback_nonexistent_entry_returns_error() {
        let mut journal = RollbackJournal::new(50);
        let result = journal.rollback(999);
        assert!(matches!(
            result,
            Err(RollbackError::EntryNotFound { id: 999 })
        ));
    }

    // rtmx:req REQ-HITL-005
    #[test]
    fn test_recent_returns_newest_first() {
        let mut journal = RollbackJournal::new(50);
        let dir = tempfile::tempdir().unwrap();

        for i in 0..3 {
            let file = dir.path().join(format!("f{i}.txt"));
            std::fs::write(&file, "x").unwrap();
            journal.snapshot(&[file.as_path()], &format!("op-{i}"));
        }

        let recent = journal.recent(2);
        assert_eq!(recent.len(), 2);
        // Newest first: id 3, then id 2.
        assert_eq!(recent[0].id, 3);
        assert_eq!(recent[1].id, 2);
    }

    // rtmx:req REQ-HITL-005
    #[test]
    fn test_empty_journal() {
        let journal = RollbackJournal::new(50);
        assert_eq!(journal.len(), 0);
        assert!(journal.is_empty());
    }

    // rtmx:req REQ-HITL-005
    #[test]
    fn test_snapshot_multiple_files() {
        let dir = tempfile::tempdir().unwrap();
        let file_a = dir.path().join("a.txt");
        let file_b = dir.path().join("b.txt");
        let file_c = dir.path().join("c.txt");
        std::fs::write(&file_a, "aaa").unwrap();
        std::fs::write(&file_b, "bbb").unwrap();
        std::fs::write(&file_c, "ccc").unwrap();

        let mut journal = RollbackJournal::new(50);
        journal.snapshot(
            &[file_a.as_path(), file_b.as_path(), file_c.as_path()],
            "write_file (3 files)",
        );

        assert_eq!(journal.len(), 1);
        let entry = &journal.entries[0];
        assert_eq!(entry.snapshots.len(), 3);
        assert_eq!(entry.snapshots[0].previous_contents.as_deref(), Some("aaa"));
        assert_eq!(entry.snapshots[1].previous_contents.as_deref(), Some("bbb"));
        assert_eq!(entry.snapshots[2].previous_contents.as_deref(), Some("ccc"));
    }
}
