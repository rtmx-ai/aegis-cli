#!/usr/bin/env python3
"""Update RTM database status from cargo test results.

This bridges cargo test (Rust) to the RTM database. rtmx natively
supports `from-go` for Go tests but has no Rust integration. The
`rtmx-from-cargo.sh` script tries to use `from-go` but the JSON format
isn't accepted.

This tool does the work directly:
  1. Run `rtmx from-tests . --show-all` to get marker -> test mapping
  2. Run `cargo test --workspace --no-fail-fast` to get pass/fail
  3. For each MISSING requirement whose linked test passed, flip to
     COMPLETE in the database
  4. Write the database back atomically

Usage:
  python3 scripts/rtmx-update-from-tests.py             # update
  python3 scripts/rtmx-update-from-tests.py --dry-run   # preview
"""

from __future__ import annotations

import argparse
import csv
import datetime
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

PROJECT_ROOT = Path(__file__).resolve().parent.parent
DB_PATH = PROJECT_ROOT / ".rtmx" / "database.csv"

REQ_RE = re.compile(r"^[\u2713\u2717\u26a0]\s+(REQ-[A-Z]+-\d+)")
TEST_RE = re.compile(r"^test ([\w:]+) \.\.\. (ok|FAILED)$")


def run(cmd: list[str], capture: bool = True) -> str:
    """Run a command, return stdout."""
    result = subprocess.run(
        cmd,
        cwd=PROJECT_ROOT,
        capture_output=capture,
        text=True,
        check=False,
    )
    return result.stdout + result.stderr


def get_test_results() -> dict[str, bool]:
    """Run cargo test and parse pass/fail per test name."""
    print("Running cargo test...", file=sys.stderr)
    output = run(["cargo", "test", "--workspace", "--no-fail-fast"])
    results: dict[str, bool] = {}
    for line in output.splitlines():
        m = TEST_RE.match(line.strip())
        if m:
            results[m.group(1)] = m.group(2) == "ok"
    print(f"  Parsed {len(results)} test results", file=sys.stderr)
    return results


def get_marker_map() -> dict[str, list[str]]:
    """Run rtmx from-tests and parse req_id -> test paths."""
    print("Scanning test markers...", file=sys.stderr)
    output = run(["rtmx", "--no-color", "from-tests", ".", "--show-all"])
    markers: dict[str, list[str]] = {}
    current_req: str | None = None
    for line in output.splitlines():
        m = REQ_RE.match(line)
        if m:
            current_req = m.group(1)
            markers.setdefault(current_req, [])
            continue
        if current_req and "::" in line:
            markers[current_req].append(line.strip())
    print(f"  Found markers for {len(markers)} requirements", file=sys.stderr)
    return markers


def normalize_test_name(marker_path: str) -> list[str]:
    """Generate candidate cargo-style names from a marker path.

    Markers look like: crates/aegis-agent/src/loop_runner.rs::tests::test_foo
    Cargo emits: loop_runner::tests::test_foo
    Integration tests emit: test_foo (no module prefix).
    """
    parts = marker_path.split("::")
    if len(parts) < 2:
        return []
    file_path = parts[0]
    file_stem = file_path.rsplit("/", 1)[-1].rsplit(".", 1)[0]
    test_path_inner = "::".join(parts[1:])
    return [
        f"{file_stem}::{test_path_inner}",  # loop_runner::tests::test_foo
        test_path_inner,                     # tests::test_foo
        parts[-1],                           # test_foo (integration tests)
    ]


def all_tests_pass(test_paths: list[str], test_results: dict[str, bool]) -> bool:
    """True if every linked test for this req has a matching cargo result and passed."""
    matched_any = False
    for marker_path in test_paths:
        for cand in normalize_test_name(marker_path):
            if cand in test_results:
                matched_any = True
                if not test_results[cand]:
                    return False
                break
        else:
            # No candidate matched -- can't verify this test ran at all
            return False
    return matched_any


def update_database(
    markers: dict[str, list[str]],
    test_results: dict[str, bool],
    dry_run: bool,
) -> int:
    """Read DB, flip MISSING -> COMPLETE for reqs whose linked tests all pass."""
    today = datetime.date.today().isoformat()

    with open(DB_PATH, newline="", encoding="utf-8") as f:
        reader = csv.DictReader(f)
        fieldnames = reader.fieldnames
        rows = list(reader)

    updates = 0
    for row in rows:
        req_id = row["req_id"]
        if row["status"] != "MISSING":
            continue
        test_paths = markers.get(req_id, [])
        if not test_paths:
            continue
        if all_tests_pass(test_paths, test_results):
            print(f"  FLIP: {req_id} MISSING -> COMPLETE ({len(test_paths)} test(s) passed)")
            row["status"] = "COMPLETE"
            if not row.get("started_date"):
                row["started_date"] = today
            row["completed_date"] = today
            updates += 1

    if updates == 0:
        print("No status changes needed.", file=sys.stderr)
        return 0

    if dry_run:
        print(f"\nDRY RUN: would flip {updates} requirements to COMPLETE", file=sys.stderr)
        return updates

    # Atomic write
    tmp = tempfile.NamedTemporaryFile(
        mode="w",
        delete=False,
        dir=DB_PATH.parent,
        prefix=".database.csv.",
        suffix=".tmp",
        encoding="utf-8",
        newline="",
    )
    try:
        writer = csv.DictWriter(tmp, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(rows)
        tmp.close()
        shutil.move(tmp.name, DB_PATH)
    except Exception:
        Path(tmp.name).unlink(missing_ok=True)
        raise

    print(f"\nUpdated {updates} requirements.", file=sys.stderr)
    return updates


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dry-run", action="store_true", help="preview changes only")
    args = parser.parse_args()

    markers = get_marker_map()
    test_results = get_test_results()
    return 0 if update_database(markers, test_results, args.dry_run) >= 0 else 1


if __name__ == "__main__":
    sys.exit(main())
