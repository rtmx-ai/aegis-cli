#!/usr/bin/env python3
"""Downgrade COMPLETE requirements whose linked tests now fail.

This is the inverse of rtmx-update-from-tests.py -- it handles
regressions. For each requirement marked COMPLETE in the RTM database,
if its linked tests now fail, the status is downgraded to PARTIAL.

Steps:
  1. Run `cargo test --workspace --no-fail-fast` and parse results
  2. Run `rtmx from-tests . --show-all` to get req -> test mapping
  3. For each COMPLETE requirement whose linked tests fail,
     downgrade to PARTIAL with a note indicating which tests failed
  4. Write the database back atomically

Usage:
  python3 scripts/test-failure-downgrade.py             # downgrade
  python3 scripts/test-failure-downgrade.py --dry-run   # preview only
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
    """Run a command, return stdout + stderr."""
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
    print(
        f"  Found markers for {len(markers)} requirements",
        file=sys.stderr,
    )
    return markers


def normalize_test_name(marker_path: str) -> list[str]:
    """Generate candidate cargo-style names from a marker path.

    Markers look like: crates/aegis-agent/src/loop_runner.rs::tests::test_foo
    Cargo emits: loop_runner::tests::test_foo
    """
    parts = marker_path.split("::")
    if len(parts) < 2:
        return []
    file_path = parts[0]
    file_stem = file_path.rsplit("/", 1)[-1].rsplit(".", 1)[0]
    test_path_inner = "::".join(parts[1:])
    return [
        f"{file_stem}::{test_path_inner}",
        test_path_inner,
        parts[-1],
    ]


def find_failing_tests(
    test_paths: list[str],
    test_results: dict[str, bool],
) -> list[str]:
    """Return names of tests that are linked and failing."""
    failures: list[str] = []
    for marker_path in test_paths:
        for cand in normalize_test_name(marker_path):
            if cand in test_results:
                if not test_results[cand]:
                    failures.append(cand)
                break
    return failures


def downgrade_database(
    markers: dict[str, list[str]],
    test_results: dict[str, bool],
    dry_run: bool,
) -> int:
    """Read DB, flip COMPLETE -> PARTIAL for reqs whose tests fail."""
    today = datetime.date.today().isoformat()

    with open(DB_PATH, newline="", encoding="utf-8") as f:
        reader = csv.DictReader(f)
        fieldnames = reader.fieldnames
        rows = list(reader)

    downgrades = 0
    for row in rows:
        req_id = row["req_id"]
        if row["status"] != "COMPLETE":
            continue
        test_paths = markers.get(req_id, [])
        if not test_paths:
            continue

        failing = find_failing_tests(test_paths, test_results)
        if failing:
            failing_names = ", ".join(failing)
            print(
                f"  DOWNGRADE: {req_id} COMPLETE -> PARTIAL "
                f"({len(failing)} test(s) failing: {failing_names})"
            )
            row["status"] = "PARTIAL"
            existing_notes = row.get("notes", "").strip()
            downgrade_note = (
                f"[{today}] Downgraded: tests failing: {failing_names}"
            )
            if existing_notes:
                row["notes"] = f"{existing_notes}; {downgrade_note}"
            else:
                row["notes"] = downgrade_note
            downgrades += 1

    if downgrades == 0:
        print("No downgrades needed.", file=sys.stderr)
        return 0

    if dry_run:
        print(
            f"\nDRY RUN: would downgrade {downgrades} requirements "
            f"to PARTIAL",
            file=sys.stderr,
        )
        return downgrades

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

    print(
        f"\nDowngraded {downgrades} requirements to PARTIAL.",
        file=sys.stderr,
    )
    return downgrades


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="preview downgrades without writing",
    )
    args = parser.parse_args()

    markers = get_marker_map()
    test_results = get_test_results()
    return (
        0 if downgrade_database(markers, test_results, args.dry_run) >= 0
        else 1
    )


if __name__ == "__main__":
    sys.exit(main())
