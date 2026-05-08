#!/usr/bin/env bash
# Generate an RTMX results JSON from cargo test output and import via rtmx from-go.
#
# Process:
#   1. Run rtmx from-tests to get marker->test mapping (req_id -> test paths)
#   2. Run cargo test --workspace --lib and capture per-test pass/fail
#   3. For each linked (req, test) pair where the test ran, emit a result
#   4. Pipe to rtmx from-go --update
#
# Usage: ./scripts/rtmx-from-cargo.sh [--dry-run]

set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_DIR"

DRY_RUN=""
if [ "${1:-}" = "--dry-run" ]; then
    DRY_RUN="--dry-run"
fi

WORKDIR="$(mktemp -d -t rtmx-bridge.XXXXXX)"
trap 'rm -rf "$WORKDIR"' EXIT

MARKERS_RAW="$WORKDIR/markers.txt"
TEST_OUTPUT="$WORKDIR/cargo-test.txt"
RESULTS_JSON="$WORKDIR/results.json"

# Step 1: marker->test mapping (scan all source dirs)
echo "Scanning test markers..." >&2
rtmx --no-color from-tests . --show-all > "$MARKERS_RAW" 2>&1

# Step 2: cargo test (lib + integration tests)
echo "Running cargo test..." >&2
cargo test --workspace --no-fail-fast > "$TEST_OUTPUT" 2>&1 || true

# Step 3: build results JSON via Python (cross-platform regex)
python3 - "$MARKERS_RAW" "$TEST_OUTPUT" "$RESULTS_JSON" <<'PYEOF'
import json
import re
import sys

markers_path, test_output_path, results_path = sys.argv[1:4]

# Parse cargo test output: "test loop_runner::tests::test_agent_config_defaults ... ok"
test_results = {}
test_re = re.compile(r"^test ([\w:]+) \.\.\. (ok|FAILED)$")
with open(test_output_path) as f:
    for line in f:
        m = test_re.match(line.strip())
        if m:
            test_name, status = m.group(1), m.group(2)
            test_results[test_name] = (status == "ok")

# Parse rtmx from-tests output:
#   ✓ REQ-AGENT-001 (6 test(s))
#       crates/aegis-agent/src/loop_runner.rs::tests::test_agent_config_defaults
req_re = re.compile(r"^[✓✗⚠]\s+(REQ-[A-Z]+-\d+)")
results = []
current_req = None

with open(markers_path) as f:
    for line in f:
        line = line.rstrip()
        m = req_re.match(line)
        if m:
            current_req = m.group(1)
            continue
        if current_req and "::" in line:
            # e.g. "    crates/aegis-agent/src/loop_runner.rs::tests::test_agent_config_defaults"
            test_path = line.strip()
            parts = test_path.split("::")
            if len(parts) < 2:
                continue
            # Extract the rust source file name without .rs extension.
            # First part is the file path; everything after is the test path within that file.
            file_path = parts[0]  # e.g. crates/aegis-agent/src/loop_runner.rs
            file_stem = file_path.rsplit("/", 1)[-1].rsplit(".", 1)[0]  # loop_runner
            test_path_inner = "::".join(parts[1:])  # tests::test_agent_config_defaults

            # Derive the Rust module path from the file path.
            # e.g. crates/aegis-tui/src/app/input_handler.rs -> app::input_handler
            mod_path = file_stem
            if "/src/" in file_path:
                after_src = file_path.split("/src/", 1)[1]  # app/input_handler.rs
                after_src = after_src.rsplit(".", 1)[0]      # app/input_handler
                mod_path = after_src.replace("/", "::")      # app::input_handler

            # cargo test prints "<mod_path>::tests::<test_name>"
            candidates = [
                f"{mod_path}::{test_path_inner}",   # app::input_handler::tests::test_...
                f"{file_stem}::{test_path_inner}",   # input_handler::tests::test_...
                test_path_inner,                      # tests::test_...
                parts[-1],                            # test_...
            ]
            for cand in candidates:
                if cand in test_results:
                    results.append({
                        "req_id": current_req,
                        "test_name": cand,
                        "test_file": file_path,
                        "passed": test_results[cand],
                    })
                    break

with open(results_path, "w") as f:
    json.dump(results, f, indent=2)

print(f"Generated {len(results)} results", file=sys.stderr)
PYEOF

LINES=$(jq 'length' "$RESULTS_JSON")
echo "Generated $LINES results in $RESULTS_JSON" >&2

if [ "$LINES" -gt 0 ]; then
    rtmx verify --results "$RESULTS_JSON" --update --force $DRY_RUN 2>&1 | tail -20
else
    echo "No results to import." >&2
    exit 1
fi
