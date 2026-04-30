//! Conversation export to JSONL format (REQ-AGENT-016).
//!
//! Exports the conversation history as newline-delimited JSON, with one
//! JSON object per line. Each entry captures the role, content, and
//! optional tool call/result metadata.

use aegis_domain::ports::Message;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::Path;

/// A single exported conversation entry.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportEntry {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_result: Option<serde_json::Value>,
}

/// Convert a domain `Message` into an `ExportEntry`.
fn message_to_entry(msg: &Message) -> ExportEntry {
    ExportEntry {
        role: format!("{:?}", msg.role),
        content: msg.content.clone(),
        tool_call: None,
        tool_result: None,
    }
}

/// Export conversation messages to JSONL format, writing one JSON object
/// per line. Returns the number of entries written.
pub fn export_to_jsonl(messages: &[Message], writer: &mut dyn io::Write) -> io::Result<usize> {
    let mut count = 0;
    for msg in messages {
        let entry = message_to_entry(msg);
        serde_json::to_writer(&mut *writer, &entry)?;
        writeln!(writer)?;
        count += 1;
    }
    Ok(count)
}

/// Export conversation to a file path. Creates parent directories if needed.
pub fn export_to_file(messages: &[Message], path: &Path) -> io::Result<usize> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::File::create(path)?;
    let mut writer = io::BufWriter::new(file);
    export_to_jsonl(messages, &mut writer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_domain::ports::Role;

    fn make_message(role: Role, content: &str) -> Message {
        Message {
            role,
            content: content.to_string(),
            cache_control: None,
        }
    }

    // rtmx:req REQ-AGENT-016
    #[test]
    fn export_empty_returns_zero() {
        let messages: Vec<Message> = vec![];
        let mut buf = Vec::new();
        let count = export_to_jsonl(&messages, &mut buf).unwrap();
        assert_eq!(count, 0);
        assert!(buf.is_empty());
    }

    // rtmx:req REQ-AGENT-016
    #[test]
    fn export_single_message() {
        let messages = vec![make_message(Role::User, "hello")];
        let mut buf = Vec::new();
        let count = export_to_jsonl(&messages, &mut buf).unwrap();
        assert_eq!(count, 1);
        let output = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 1);
    }

    // rtmx:req REQ-AGENT-016
    #[test]
    fn export_roundtrip_parseable() {
        let messages = vec![
            make_message(Role::User, "hello"),
            make_message(Role::Assistant, "world"),
            make_message(Role::Tool, "result"),
        ];
        let mut buf = Vec::new();
        export_to_jsonl(&messages, &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        for line in output.lines() {
            let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
            assert!(parsed.is_object());
        }
    }

    // rtmx:req REQ-AGENT-016
    #[test]
    fn export_to_file_creates_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("export.jsonl");
        let messages = vec![make_message(Role::User, "test")];
        let count = export_to_file(&messages, &path).unwrap();
        assert_eq!(count, 1);
        assert!(path.exists());
    }

    // rtmx:req REQ-AGENT-016
    #[test]
    fn export_preserves_role_and_content() {
        let messages = vec![make_message(Role::Assistant, "analysis complete")];
        let mut buf = Vec::new();
        export_to_jsonl(&messages, &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        let entry: ExportEntry = serde_json::from_str(output.lines().next().unwrap()).unwrap();
        assert_eq!(entry.role, "Assistant");
        assert_eq!(entry.content, "analysis complete");
        assert!(entry.tool_call.is_none());
        assert!(entry.tool_result.is_none());
    }
}
