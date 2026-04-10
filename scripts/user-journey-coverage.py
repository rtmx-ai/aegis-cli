#!/usr/bin/env python3
"""Report user journey coverage metric.

Defines key user journeys and checks which E2E tests exist for each.
Reports a coverage percentage.

User journeys:
  1. first-run   -- aegis init / onboarding
  2. chat        -- interactive chat session with LLM
  3. hitl        -- human-in-the-loop approval flow
  4. session     -- session restore / persistence
  5. plugin      -- infrastructure plugin provisioning

For each journey, checks:
  - Feature files in tests/features/ with matching category
  - Integration tests in crates/aegis-cli/tests/
  - RTM database entries with E2E or BDD validation_method

Usage:
  python3 scripts/user-journey-coverage.py
"""

from __future__ import annotations

import csv
import sys
from pathlib import Path

PROJECT_ROOT = Path(__file__).resolve().parent.parent
FEATURES_DIR = PROJECT_ROOT / "tests" / "features"
TESTS_DIR = PROJECT_ROOT / "crates" / "aegis-cli" / "tests"
DB_PATH = PROJECT_ROOT / ".rtmx" / "database.csv"

# Map journey name -> (feature subdirs, test file patterns, RTM categories)
JOURNEYS: dict[str, dict[str, list[str]]] = {
    "first-run": {
        "feature_dirs": ["onboard"],
        "test_patterns": ["onboard", "init"],
        "rtm_categories": ["ONBOARD"],
    },
    "chat": {
        "feature_dirs": ["tui", "agent"],
        "test_patterns": ["chat", "tui", "agent"],
        "rtm_categories": ["TUI", "AGENT"],
    },
    "hitl": {
        "feature_dirs": ["hitl"],
        "test_patterns": ["hitl", "approval"],
        "rtm_categories": ["HITL"],
    },
    "session": {
        "feature_dirs": ["audit"],
        "test_patterns": ["session", "audit"],
        "rtm_categories": ["AUDIT"],
    },
    "plugin": {
        "feature_dirs": ["infra"],
        "test_patterns": ["infra", "plugin"],
        "rtm_categories": ["INFRA"],
    },
}


def check_feature_files(dirs: list[str]) -> int:
    """Count feature files in given subdirectories."""
    count = 0
    for d in dirs:
        feature_dir = FEATURES_DIR / d
        if feature_dir.exists():
            count += len(list(feature_dir.rglob("*.feature")))
    return count


def check_test_files(patterns: list[str]) -> int:
    """Count test files matching patterns."""
    count = 0
    if not TESTS_DIR.exists():
        return count
    for test_file in TESTS_DIR.glob("*.rs"):
        name = test_file.stem.lower()
        for pattern in patterns:
            if pattern in name:
                count += 1
                break
    return count


def check_rtm_entries(categories: list[str]) -> tuple[int, int]:
    """Count RTM entries and how many have tests, for given categories."""
    if not DB_PATH.exists():
        return 0, 0
    total = 0
    with_tests = 0
    with open(DB_PATH, newline="", encoding="utf-8") as f:
        reader = csv.DictReader(f)
        for row in reader:
            cat = row.get("category", "").strip()
            if cat in categories:
                total += 1
                test_module = row.get("test_module", "").strip()
                if test_module:
                    with_tests += 1
    return total, with_tests


def main() -> int:
    print("User Journey Coverage Report")
    print("=" * 60)
    print()

    covered_journeys = 0
    total_journeys = len(JOURNEYS)

    for name, config in JOURNEYS.items():
        features = check_feature_files(config["feature_dirs"])
        tests = check_test_files(config["test_patterns"])
        rtm_total, rtm_tested = check_rtm_entries(
            config["rtm_categories"]
        )

        has_coverage = features > 0 or tests > 0
        if has_coverage:
            covered_journeys += 1

        status = "COVERED" if has_coverage else "MISSING"
        print(f"Journey: {name}")
        print(f"  Status:       {status}")
        print(f"  Features:     {features} file(s)")
        print(f"  Tests:        {tests} file(s)")
        print(f"  RTM entries:  {rtm_tested}/{rtm_total} with tests")
        print()

    pct = (covered_journeys / total_journeys * 100) if total_journeys > 0 else 0.0
    print("=" * 60)
    print(
        f"Journey coverage: {covered_journeys}/{total_journeys} "
        f"({pct:.0f}%)"
    )

    return 0


if __name__ == "__main__":
    sys.exit(main())
