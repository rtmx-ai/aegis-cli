//! Session persistence: save and restore chat sessions across restarts.
//!
//! Sessions are stored as JSONL files at `~/.aegis/sessions/<uuid>.jsonl`.
//! Each line is a JSON object with role, content, and ISO 8601 timestamp.

use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::SystemTime;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::messages::{ChatMessage, MessageKind};

/// Persisted representation of a single chat message (one JSONL line).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedMessage {
    pub role: String,
    pub content: String,
    pub timestamp: String,
}

/// Summary of an available session for the resume prompt.
#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub session_id: String,
    pub last_modified: SystemTime,
    pub message_count: usize,
}

/// Manages reading and writing session JSONL files.
pub struct SessionStore {
    sessions_dir: PathBuf,
}

impl SessionStore {
    pub fn new(sessions_dir: PathBuf) -> Self {
        Self { sessions_dir }
    }

    /// Append a single message to a session file.
    ///
    /// Creates the sessions directory and the file if they do not exist.
    pub fn save_message(&self, session_id: &str, message: &ChatMessage) -> io::Result<()> {
        fs::create_dir_all(&self.sessions_dir)?;

        let persisted = PersistedMessage {
            role: role_from_kind(&message.kind),
            content: message.content.clone(),
            timestamp: Utc::now().to_rfc3339(),
        };

        let mut line = serde_json::to_string(&persisted)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        line.push('\n');

        let path = self.session_path(session_id);

        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        file.write_all(line.as_bytes())?;

        Ok(())
    }

    /// Load all messages from a session file.
    ///
    /// Returns an empty `Vec` if the file does not exist.
    pub fn load_session(&self, session_id: &str) -> io::Result<Vec<ChatMessage>> {
        let path = self.session_path(session_id);
        if !path.exists() {
            return Ok(Vec::new());
        }

        let data = fs::read_to_string(&path)?;
        let mut messages = Vec::new();

        for line in data.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let persisted: PersistedMessage = serde_json::from_str(line)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            messages.push(chat_message_from_persisted(&persisted));
        }

        Ok(messages)
    }

    /// List all available sessions, sorted by last-modified (most recent first).
    pub fn list_sessions(&self) -> io::Result<Vec<SessionSummary>> {
        if !self.sessions_dir.exists() {
            return Ok(Vec::new());
        }

        let mut summaries = Vec::new();

        for entry in fs::read_dir(&self.sessions_dir)? {
            let entry = entry?;
            let path = entry.path();

            let name = match path.file_stem().and_then(|s| s.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };

            let ext = path.extension().and_then(|s| s.to_str());
            if ext != Some("jsonl") {
                continue;
            }

            let metadata = fs::metadata(&path)?;
            let last_modified = metadata.modified()?;

            let data = fs::read_to_string(&path)?;
            let message_count = data.lines().filter(|l| !l.trim().is_empty()).count();

            summaries.push(SessionSummary {
                session_id: name,
                last_modified,
                message_count,
            });
        }

        summaries.sort_by_key(|s| std::cmp::Reverse(s.last_modified));

        Ok(summaries)
    }

    /// Return the most recently modified session, if any.
    pub fn most_recent_session(&self) -> io::Result<Option<SessionSummary>> {
        let sessions = self.list_sessions()?;
        Ok(sessions.into_iter().next())
    }

    /// Delete a session file.
    pub fn delete_session(&self, session_id: &str) -> io::Result<()> {
        let path = self.session_path(session_id);
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    fn session_path(&self, session_id: &str) -> PathBuf {
        self.sessions_dir.join(format!("{session_id}.jsonl"))
    }
}

fn role_from_kind(kind: &MessageKind) -> String {
    match kind {
        MessageKind::User => "user".to_string(),
        MessageKind::Assistant => "assistant".to_string(),
        MessageKind::System => "system".to_string(),
        MessageKind::ToolCall { .. } => "assistant".to_string(),
        MessageKind::ToolResult => "assistant".to_string(),
        MessageKind::Error => "system".to_string(),
    }
}

fn chat_message_from_persisted(p: &PersistedMessage) -> ChatMessage {
    match p.role.as_str() {
        "user" => ChatMessage::user(&p.content),
        "assistant" => ChatMessage::assistant(&p.content),
        _ => ChatMessage::system(&p.content),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_store(dir: &std::path::Path) -> SessionStore {
        SessionStore::new(dir.to_path_buf())
    }

    // rtmx:req REQ-TUI-011
    #[test]
    fn test_session_persist_and_restore() {
        let tmp = tempfile::tempdir().unwrap();
        let store = make_store(tmp.path());

        let msg1 = ChatMessage::user("Hello world");
        let msg2 = ChatMessage::assistant("Hi there");
        let msg3 = ChatMessage::system("Session started");

        store.save_message("sess-1", &msg1).unwrap();
        store.save_message("sess-1", &msg2).unwrap();
        store.save_message("sess-1", &msg3).unwrap();

        let loaded = store.load_session("sess-1").unwrap();
        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded[0].content, "Hello world");
        assert_eq!(loaded[0].kind, MessageKind::User);
        assert_eq!(loaded[1].content, "Hi there");
        assert_eq!(loaded[1].kind, MessageKind::Assistant);
        assert_eq!(loaded[2].content, "Session started");
        assert_eq!(loaded[2].kind, MessageKind::System);
    }

    // rtmx:req REQ-TUI-011
    #[test]
    fn test_session_list_returns_sessions() {
        let tmp = tempfile::tempdir().unwrap();
        let store = make_store(tmp.path());

        store
            .save_message("aaa", &ChatMessage::user("first"))
            .unwrap();

        // Ensure different modified times by writing a second session after.
        store
            .save_message("bbb", &ChatMessage::user("second"))
            .unwrap();
        // Add another message to bbb so it's definitely newer.
        store
            .save_message("bbb", &ChatMessage::user("second-2"))
            .unwrap();

        let sessions = store.list_sessions().unwrap();
        assert_eq!(sessions.len(), 2);
        // Most recent first -- bbb was written last.
        assert_eq!(sessions[0].session_id, "bbb");
        assert_eq!(sessions[0].message_count, 2);
        assert_eq!(sessions[1].session_id, "aaa");
        assert_eq!(sessions[1].message_count, 1);
    }

    // rtmx:req REQ-TUI-011
    #[test]
    fn test_most_recent_session() {
        let tmp = tempfile::tempdir().unwrap();
        let store = make_store(tmp.path());

        store
            .save_message("old", &ChatMessage::user("old msg"))
            .unwrap();
        store
            .save_message("new", &ChatMessage::user("new msg"))
            .unwrap();

        let recent = store.most_recent_session().unwrap().unwrap();
        assert_eq!(recent.session_id, "new");
    }

    // rtmx:req REQ-TUI-011
    #[test]
    fn test_session_delete() {
        let tmp = tempfile::tempdir().unwrap();
        let store = make_store(tmp.path());

        store
            .save_message("del-me", &ChatMessage::user("bye"))
            .unwrap();
        assert_eq!(store.list_sessions().unwrap().len(), 1);

        store.delete_session("del-me").unwrap();
        assert_eq!(store.list_sessions().unwrap().len(), 0);

        let loaded = store.load_session("del-me").unwrap();
        assert!(loaded.is_empty());
    }

    // rtmx:req REQ-TUI-011
    #[test]
    fn test_load_nonexistent_session() {
        let tmp = tempfile::tempdir().unwrap();
        let store = make_store(tmp.path());

        let loaded = store.load_session("does-not-exist").unwrap();
        assert!(loaded.is_empty());
    }

    // rtmx:req REQ-TUI-011
    #[test]
    fn test_save_creates_sessions_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let nested = tmp.path().join("deep").join("sessions");
        let store = make_store(&nested);

        assert!(!nested.exists());
        store
            .save_message("auto-dir", &ChatMessage::user("hi"))
            .unwrap();
        assert!(nested.exists());
        assert_eq!(store.load_session("auto-dir").unwrap().len(), 1);
    }
}
