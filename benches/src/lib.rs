// rtmx:req REQ-TEST-048
//! Benchmark crate -- the actual benchmarks live in benches/benchmark.rs.
//! This lib.rs exists only to satisfy Cargo's package target requirement.

#[cfg(test)]
mod tests {
    // rtmx:req REQ-TEST-048
    #[test]
    fn test_benchmark_harness_exists() {
        // Verify the benchmark binary exists and the criterion harness is
        // configured. The actual benchmarks run via `cargo bench`.
        let bench_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("benchmark.rs");
        assert!(
            bench_file.exists(),
            "benchmark.rs must exist in the benches crate"
        );
    }

    // rtmx:req REQ-TEST-050
    #[test]
    fn test_baseline_script_exists() {
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("scripts/bench-baseline.sh");
        assert!(
            script.exists(),
            "scripts/bench-baseline.sh must exist at repo root"
        );
        // Verify it is executable (Unix only).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::metadata(&script).unwrap().permissions();
            assert!(
                perms.mode() & 0o111 != 0,
                "bench-baseline.sh must be executable"
            );
        }
        // Verify it supports save and compare modes.
        let content = std::fs::read_to_string(&script).unwrap();
        assert!(content.contains("save"), "script must support save mode");
        assert!(
            content.contains("compare"),
            "script must support compare mode"
        );
    }

    // rtmx:req REQ-TEST-049
    #[test]
    fn test_benchmark_suite_covers_hot_paths() {
        // Verify the benchmark file references all three required hot paths.
        let bench_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("benchmark.rs");
        let content = std::fs::read_to_string(&bench_file).unwrap();
        assert!(
            content.contains("rtm_csv_parse"),
            "benchmark must cover RTM CSV parse"
        );
        assert!(
            content.contains("audit_ledger_append"),
            "benchmark must cover audit ledger append"
        );
        assert!(
            content.contains("tool_dispatch"),
            "benchmark must cover tool dispatch"
        );
    }
}
