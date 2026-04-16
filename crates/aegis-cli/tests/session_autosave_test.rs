//! Session autosave behavior tests (REQ-TUI-060).
//!
//! Background: aegis previously only wrote the session snapshot on clean
//! shutdown (SIGTERM handler at the bottom of `run_interactive_chat`). If the
//! process was killed mid-conversation (dev-run.sh hot reload, panic, OOM)
//! the running session was lost, because the on-exit save path never ran.
//!
//! The autosave contract is:
//!
//! 1. After every completed assistant turn (TUI `AgentDone` event) the event
//!    loop calls `save_session` so the latest message history hits disk.
//! 2. A 30-second background timer also triggers a save if the session is
//!    "dirty" (new messages since the last save) even without an `AgentDone`
//!    event (e.g. approval-gated turn still streaming, user idle).
//! 3. Each save is atomic: `aegis_agent::session::save_session` writes to a
//!    sibling tmp file then renames into place, so readers never see a torn
//!    snapshot.
//!
//! The tests below exercise the building blocks the autosave path uses --
//! since `save_session_now` lives inside the aegis-cli binary (main.rs), we
//! cannot import it directly. Instead we drive the same underlying API
//! (`save_session`) through the sequence of states the binary would produce,
//! and verify the contract invariants hold at the filesystem level.

use aegis_agent::session::{SessionSnapshot, load_session, save_session};
use aegis_domain::ports::{Message, Role};
use std::path::PathBuf;

/// Build a snapshot mimicking what `build_snapshot_from_app` produces in the
/// TUI binary after N user/assistant turns.
fn snapshot_after_turns(session_id: &str, turns: usize) -> SessionSnapshot {
    let mut messages = Vec::new();
    for i in 0..turns {
        messages.push(Message {
            role: Role::User,
            content: format!("user turn {i}"),
        });
        messages.push(Message {
            role: Role::Assistant,
            content: format!("assistant response {i}"),
        });
    }
    SessionSnapshot::new(
        session_id,
        messages,
        (turns as u64) * 100,
        (turns as u64) * 50,
        "claude-opus-4-6",
        PathBuf::from("/tmp/autosave-test"),
    )
}

// rtmx:req REQ-TUI-060
/// Autosave must write a fresh snapshot after every assistant turn so a crash
/// mid-conversation doesn't roll the session back to the previous turn.
#[test]
fn test_session_autosaves_during_conversation() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let sessions_dir = tmp.path().to_path_buf();
    let session_id = "sess-autosave-001";

    // Simulate three completed assistant turns. Each `AgentDone` in the event
    // loop triggers `save_session_now`, which calls `save_session` here.
    let mut last_timestamp = 0u64;
    let mut saved_path = None;
    for turn in 1..=3 {
        let snap = snapshot_after_turns(session_id, turn);
        let path = save_session(&sessions_dir, &snap).expect("save snapshot");
        saved_path = Some(path.clone());

        // The file MUST exist after each save, and loading it back must yield
        // the latest turn count. No torn writes, no lingering tmp files.
        let loaded = load_session(&path).expect("snapshot loadable");
        assert_eq!(
            loaded.messages.len(),
            turn * 2,
            "after turn {turn} snapshot should contain {n} messages",
            n = turn * 2,
        );
        assert_eq!(loaded.session_id, session_id);

        // Timestamp must be monotonic so the "latest save" is observable.
        assert!(
            loaded.timestamp >= last_timestamp,
            "timestamp must not regress: prev={prev} now={now}",
            prev = last_timestamp,
            now = loaded.timestamp,
        );
        last_timestamp = loaded.timestamp;

        // The atomic-rename tmp file must not linger.
        let tmp_file = sessions_dir.join(format!(".{session_id}.json.tmp"));
        assert!(
            !tmp_file.exists(),
            "atomic rename must not leave a .tmp file behind after turn {turn}",
        );
    }

    // Final state: three turns' worth of messages are on disk at the final
    // snapshot path, reachable via the `current.json` pointer too.
    let final_path = saved_path.expect("at least one save happened");
    let final_snap = load_session(&final_path).expect("final load");
    assert_eq!(final_snap.messages.len(), 6);
    assert_eq!(final_snap.input_tokens, 300);
    assert_eq!(final_snap.output_tokens, 150);
}

// rtmx:req REQ-TUI-060
/// The save path must be atomic: an interrupted write cannot leave a
/// half-written session file that would then fail to parse on the next
/// startup. `save_session` writes to a sibling tmp file and renames into
/// place, so the only observable states are "old snapshot" and "new
/// snapshot" -- never "half of new snapshot".
#[test]
fn test_save_helper_writes_atomically() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let sessions_dir = tmp.path().to_path_buf();
    let session_id = "sess-atomic-001";

    // First save: establishes a baseline snapshot on disk.
    let snap_v1 = snapshot_after_turns(session_id, 1);
    let path = save_session(&sessions_dir, &snap_v1).expect("first save");
    assert!(path.exists(), "baseline snapshot must exist");
    let loaded_v1 = load_session(&path).expect("baseline loadable");
    assert_eq!(loaded_v1.messages.len(), 2);

    // Second save with more messages: overwrites atomically. At no point
    // should a reader see a torn file, and no partial-write tmp should
    // linger after the rename.
    let snap_v2 = snapshot_after_turns(session_id, 5);
    let path2 = save_session(&sessions_dir, &snap_v2).expect("second save");
    assert_eq!(path, path2, "same session_id -> same final file path");

    // Verify the tmp file has been renamed away (no torn writes exposed).
    let tmp_file = sessions_dir.join(format!(".{session_id}.json.tmp"));
    assert!(
        !tmp_file.exists(),
        "atomic rename must not leave a .tmp file behind",
    );

    // The file on disk must be the NEW snapshot, never a mix.
    let loaded_v2 = load_session(&path2).expect("new snapshot loadable");
    assert_eq!(
        loaded_v2.messages.len(),
        10,
        "loaded snapshot must be the post-rename version, not a torn mix",
    );
}

// rtmx:req REQ-TUI-060
/// Dirty tracking: if no new messages have been added since the last save,
/// the autosave path should be a no-op (in practice this means the timer
/// tick observes `messages.len() == last_saved_count` and skips the save).
/// This test documents the invariant by checking that a repeat save of the
/// same snapshot produces an identical file (schema_version, session_id,
/// and message count are stable), so deduplication is safe to implement in
/// main.rs.
#[test]
fn test_repeat_save_is_idempotent() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let sessions_dir = tmp.path().to_path_buf();
    let session_id = "sess-idem-001";

    let snap = snapshot_after_turns(session_id, 2);
    let path_a = save_session(&sessions_dir, &snap).expect("save a");
    let loaded_a = load_session(&path_a).expect("load a");

    let path_b = save_session(&sessions_dir, &snap).expect("save b");
    let loaded_b = load_session(&path_b).expect("load b");

    assert_eq!(loaded_a.session_id, loaded_b.session_id);
    assert_eq!(loaded_a.messages, loaded_b.messages);
    assert_eq!(loaded_a.schema_version, loaded_b.schema_version);
}
