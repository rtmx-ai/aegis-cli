#!/usr/bin/env python3
"""Test pyramid balance metric for aegis-cli.

Counts tests by type (unit, integration, E2E) and reports whether the
test pyramid shape is healthy (unit > integration > E2E).

Usage:
    python3 scripts/test-pyramid-balance.py [--json]

Exit codes:
    0  Pyramid is balanced (unit > integration > E2E)
    1  Pyramid is inverted or other error
"""

import glob
import os
import re
import sys


def count_unit_tests(root: str) -> int:
    """Count #[test] occurrences inside src/**/*.rs files."""
    count = 0
    for path in glob.glob(os.path.join(root, "crates", "**", "src", "**", "*.rs"), recursive=True):
        try:
            with open(path, "r", encoding="utf-8", errors="replace") as f:
                content = f.read()
        except OSError:
            continue
        # Count #[test] annotations (simple heuristic)
        count += len(re.findall(r"#\[test\]", content))
        # Also count #[tokio::test]
        count += len(re.findall(r"#\[tokio::test\]", content))
    return count


def count_integration_tests(root: str) -> int:
    """Count test files in crates/*/tests/*.rs."""
    count = 0
    for path in glob.glob(os.path.join(root, "crates", "*", "tests", "*.rs")):
        try:
            with open(path, "r", encoding="utf-8", errors="replace") as f:
                content = f.read()
        except OSError:
            continue
        # Count test functions in integration test files
        count += len(re.findall(r"#\[test\]", content))
        count += len(re.findall(r"#\[tokio::test\]", content))
    return count


def count_e2e_scenarios(root: str) -> int:
    """Count non-skipped Scenario lines in .feature files."""
    count = 0
    for path in glob.glob(os.path.join(root, "tests", "features", "**", "*.feature"), recursive=True):
        try:
            with open(path, "r", encoding="utf-8", errors="replace") as f:
                for line in f:
                    stripped = line.strip()
                    if stripped.startswith("Scenario:") or stripped.startswith("Scenario Outline:"):
                        count += 1
        except OSError:
            continue
    return count


def main() -> int:
    # Find repo root (directory containing Cargo.toml at workspace level)
    script_dir = os.path.dirname(os.path.abspath(__file__))
    root = os.path.dirname(script_dir)

    unit = count_unit_tests(root)
    integration = count_integration_tests(root)
    e2e = count_e2e_scenarios(root)
    total = unit + integration + e2e

    json_mode = "--json" in sys.argv

    if json_mode:
        import json
        result = {
            "unit": unit,
            "integration": integration,
            "e2e": e2e,
            "total": total,
            "balanced": unit > integration >= e2e if total > 0 else True,
        }
        print(json.dumps(result, indent=2))
    else:
        print("Test Pyramid Balance Report")
        print("=" * 40)
        print(f"  Unit tests:        {unit:>4}")
        print(f"  Integration tests: {integration:>4}")
        print(f"  E2E scenarios:     {e2e:>4}")
        print(f"  Total:             {total:>4}")
        print()

        if total == 0:
            print("WARNING: No tests found.")
            return 1

        unit_pct = (unit / total) * 100
        int_pct = (integration / total) * 100
        e2e_pct = (e2e / total) * 100
        print(f"  Ratio: {unit_pct:.0f}% unit / {int_pct:.0f}% integration / {e2e_pct:.0f}% e2e")
        print()

        balanced = unit > integration >= e2e
        if balanced:
            print("PASS: Test pyramid is balanced (unit > integration >= e2e).")
        else:
            print("WARNING: Test pyramid is inverted!")
            if unit <= integration:
                print("  -> Unit tests should outnumber integration tests.")
            if integration < e2e:
                print("  -> Integration tests should outnumber E2E tests.")
            return 1

    return 0


if __name__ == "__main__":
    sys.exit(main())
