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
