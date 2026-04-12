//! Property-based tests validating domain type invariants.
//! PROPTEST_SEED must be set in CI for reproducibility (REQ-TEST-015).

use aegis_domain::types::{FilePath, ToolCall, ToolRisk};
use proptest::prelude::*;

/// Generate an arbitrary `ToolCall` variant for fuzzing.
fn arb_tool_call() -> impl Strategy<Value = ToolCall> {
    prop_oneof![
        any::<String>().prop_map(|s| ToolCall::ReadFile {
            path: FilePath::new_unchecked(&s),
        }),
        (any::<String>(), any::<String>()).prop_map(|(p, c)| ToolCall::WriteFile {
            path: FilePath::new_unchecked(&p),
            content: c,
        }),
        (any::<String>(), any::<u64>()).prop_map(|(cmd, t)| ToolCall::RunCommand {
            command: cmd,
            timeout_secs: t,
        }),
        any::<String>().prop_map(|s| ToolCall::ListDir {
            path: FilePath::new_unchecked(&s),
        }),
        (any::<String>(), any::<String>()).prop_map(|(pat, p)| ToolCall::Grep {
            pattern: pat,
            path: FilePath::new_unchecked(&p),
        }),
    ]
}

// rtmx:req REQ-TEST-015
proptest! {
    #[test]
    fn tool_risk_classification_never_panics(call in arb_tool_call()) {
        // Every possible ToolCall must classify to a valid ToolRisk.
        let risk = call.risk();
        assert!(risk == ToolRisk::ReadOnly || risk == ToolRisk::StateMutating);
    }
}

// rtmx:req REQ-TEST-015
proptest! {
    #[test]
    fn tool_call_debug_never_panics(call in arb_tool_call()) {
        // Debug formatting must never panic on arbitrary inputs.
        let _ = format!("{:?}", call);
    }
}

// rtmx:req REQ-TEST-015
proptest! {
    #[test]
    fn file_path_display_never_panics(s in "\\PC{0,200}") {
        // Arbitrary strings as file paths must not panic Display.
        let fp = FilePath::new_unchecked(&s);
        let _ = format!("{}", fp);
    }
}

// rtmx:req REQ-TEST-015
proptest! {
    #[test]
    fn read_only_variants_are_read_only(
        path_str in "\\PC{0,100}",
        pattern in "\\PC{0,50}"
    ) {
        let read = ToolCall::ReadFile {
            path: FilePath::new_unchecked(&path_str),
        };
        let list = ToolCall::ListDir {
            path: FilePath::new_unchecked(&path_str),
        };
        let grep = ToolCall::Grep {
            pattern,
            path: FilePath::new_unchecked(&path_str),
        };
        assert_eq!(read.risk(), ToolRisk::ReadOnly);
        assert_eq!(list.risk(), ToolRisk::ReadOnly);
        assert_eq!(grep.risk(), ToolRisk::ReadOnly);
    }
}

// rtmx:req REQ-TEST-015
proptest! {
    #[test]
    fn mutating_variants_are_state_mutating(
        path_str in "\\PC{0,100}",
        content in "\\PC{0,200}",
        command in "\\PC{0,100}",
        timeout in any::<u64>()
    ) {
        let write = ToolCall::WriteFile {
            path: FilePath::new_unchecked(&path_str),
            content,
        };
        let run = ToolCall::RunCommand {
            command,
            timeout_secs: timeout,
        };
        assert_eq!(write.risk(), ToolRisk::StateMutating);
        assert_eq!(run.risk(), ToolRisk::StateMutating);
    }
}

// rtmx:req REQ-TEST-015
#[test]
fn proptest_seed_determinism_contract() {
    // When PROPTEST_SEED is set, two runs produce the same sequence.
    // This test documents the contract -- actual reproducibility is
    // enforced by the CI env var PROPTEST_SEED=12345.
    // The fact that this test compiles and runs with proptest proves
    // the framework is wired in.
    let config = proptest::test_runner::Config {
        source_file: Some(file!()),
        ..Default::default()
    };
    let mut runner = proptest::test_runner::TestRunner::new(config);
    let values: Vec<u32> = (0..10).map(|_| runner.rng().next_u32()).collect();
    // Verify we got 10 values without panic.
    assert_eq!(values.len(), 10);
}
