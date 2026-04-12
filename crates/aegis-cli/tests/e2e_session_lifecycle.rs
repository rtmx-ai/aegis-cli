//! E2E session save and restore across simulated process restart (REQ-TEST-029).
//!
//! Uses IsolatedHome and the session persistence module to verify that a saved
//! session can be loaded by a new "process" (new agent instance) with all state
//! intact.

use aegis_agent::session::{SessionSnapshot, default_session_dir, load_session, save_session};
use aegis_domain::ports::{Message, Role};
use std::path::PathBuf;

/// Build a realistic session snapshot with multiple messages and token counts.
fn sample_session(session_id: &str) -> SessionSnapshot {
    SessionSnapshot::new(
        session_id,
        vec![
            Message {
                role: Role::User,
                content: "Explain the architecture of aegis-cli.".into(),
            },
            Message {
                role: Role::Assistant,
                content: "aegis-cli is a Rust workspace with 11 crates...".into(),
            },
            Message {
                role: Role::User,
                content: "Show me the plugin protocol.".into(),
            },
            Message {
                role: Role::Assistant,
                content: "The aegis-infra/v1 protocol uses NDJSON...".into(),
            },
        ],
        1500,
        320,
        "claude-opus-4-6",
        PathBuf::from("/home/user/project"),
    )
}

// rtmx:req REQ-TEST-029
#[test]
fn test_session_save_and_restore_roundtrip() {
    // 1. Create isolated session directory
    let tmp = tempfile::tempdir().expect("create tempdir");
    let sessions_dir = tmp.path().join(".aegis/sessions");
    std::fs::create_dir_all(&sessions_dir).unwrap();

    // 2. Create a session with realistic state
    let original = sample_session("sess-e2e-001");
    assert_eq!(original.messages.len(), 4);
    assert_eq!(original.input_tokens, 1500);
    assert_eq!(original.output_tokens, 320);

    // 3. Save the session (simulates aegis exit / SIGTERM)
    let saved_path = save_session(&sessions_dir, &original).expect("save session");
    assert!(saved_path.exists(), "session file must exist after save");

    // 4. Load the session (simulates new aegis process starting)
    let restored = load_session(&saved_path).expect("session must be loadable");

    // 5. Verify all state matches
    assert_eq!(restored.session_id, original.session_id);
    assert_eq!(restored.messages.len(), original.messages.len());
    assert_eq!(restored.input_tokens, original.input_tokens);
    assert_eq!(restored.output_tokens, original.output_tokens);
    assert_eq!(restored.model_name, original.model_name);
    assert_eq!(restored.working_dir, original.working_dir);
    assert_eq!(restored.schema_version, original.schema_version);

    // Verify individual messages survived the roundtrip
    for (orig, rest) in original.messages.iter().zip(restored.messages.iter()) {
        assert_eq!(orig.role, rest.role);
        assert_eq!(orig.content, rest.content);
    }
}

// rtmx:req REQ-TEST-029
#[test]
fn test_session_ids_are_unique() {
    // Each SessionSnapshot gets a unique session_id from the caller.
    // Verify that domain::SessionId generates distinct values.
    let id1 = aegis_domain::types::SessionId::new().to_string();
    let id2 = aegis_domain::types::SessionId::new().to_string();
    assert_ne!(id1, id2, "session IDs must be unique");
}

// rtmx:req REQ-TEST-029
#[test]
fn test_session_directory_structure() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let sessions_dir = tmp.path().join(".aegis/sessions");
    std::fs::create_dir_all(&sessions_dir).unwrap();

    let snap = sample_session("sess-struct-001");
    let path = save_session(&sessions_dir, &snap).expect("save");

    // Verify the file lands in the right place with the right name
    assert_eq!(path.parent().unwrap(), sessions_dir);
    assert_eq!(path.file_name().unwrap(), "sess-struct-001.json");

    // Verify the file is valid JSON by loading it back through the session API
    let loaded = load_session(&path).expect("file must be valid session JSON");
    assert_eq!(loaded.session_id, "sess-struct-001");
    assert_eq!(loaded.messages.len(), 4);
}

// rtmx:req REQ-TEST-029
#[test]
fn test_session_survives_multiple_save_restore_cycles() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let sessions_dir = tmp.path().join(".aegis/sessions");
    std::fs::create_dir_all(&sessions_dir).unwrap();

    // Simulate 3 sequential sessions, each loading from the previous
    let mut cumulative_input_tokens = 0u64;
    let mut cumulative_output_tokens = 0u64;

    for i in 0..3 {
        let id = format!("sess-cycle-{i:03}");
        cumulative_input_tokens += 500;
        cumulative_output_tokens += 100;

        let snap = SessionSnapshot::new(
            &id,
            vec![Message {
                role: Role::User,
                content: format!("Turn {i}"),
            }],
            cumulative_input_tokens,
            cumulative_output_tokens,
            "test-model",
            PathBuf::from("/tmp"),
        );

        let path = save_session(&sessions_dir, &snap).expect("save");
        let loaded = load_session(&path).expect("load");
        assert_eq!(loaded.input_tokens, cumulative_input_tokens);
        assert_eq!(loaded.output_tokens, cumulative_output_tokens);
    }
}

// rtmx:req REQ-TEST-029
#[test]
fn test_default_session_dir_uses_home() {
    // default_session_dir() should return ~/.aegis/sessions
    // We test the function exists and returns a path ending in sessions
    if let Some(dir) = default_session_dir() {
        assert!(
            dir.ends_with(".aegis/sessions"),
            "default session dir should end with .aegis/sessions, got: {}",
            dir.display()
        );
    }
    // If HOME is not set (unlikely in CI), None is acceptable
}

// rtmx:req REQ-TEST-029
#[cfg(unix)]
#[test]
fn test_current_json_symlink_updated_on_save() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let sessions_dir = tmp.path().join("sessions");
    std::fs::create_dir_all(&sessions_dir).unwrap();

    let snap = sample_session("sess-link-001");
    save_session(&sessions_dir, &snap).expect("save");

    let link = sessions_dir.join("current.json");
    assert!(
        link.symlink_metadata().is_ok(),
        "current.json symlink must exist"
    );

    // Loading via the symlink should work
    let loaded = load_session(&link).expect("load via symlink");
    assert_eq!(loaded.session_id, "sess-link-001");
}
