#!/usr/bin/env python3
"""Report BDD step definition coverage.

Scans tests/features/**/*.feature for Given/When/Then step text and
checks for matching step definitions in crates/aegis-cli/tests/steps/.

Reports:
  - Total unique step patterns found in feature files
  - Number of step definition files
  - Unmatched steps (steps without any step definition file)

This is an aspirational tool -- most steps will not have definitions yet.
The goal is to track progress toward full step coverage.

Usage:
  python3 scripts/bdd-step-coverage.py
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

PROJECT_ROOT = Path(__file__).resolve().parent.parent
FEATURES_DIR = PROJECT_ROOT / "tests" / "features"
STEPS_DIR = PROJECT_ROOT / "crates" / "aegis-cli" / "tests" / "steps"

# Match Given/When/Then/And/But step lines in feature files
STEP_PATTERN = re.compile(
    r"^\s*(Given|When|Then|And|But)\s+(.+)$", re.MULTILINE
)

# Match step definition macros in Rust files
STEP_DEF_PATTERN = re.compile(
    r"#\[(given|when|then)\((?:regex\s*=\s*)?[\"'](.+?)[\"']\)\]"
)


def scan_feature_steps() -> dict[str, set[str]]:
    """Extract step text from feature files, grouped by keyword."""
    steps: dict[str, set[str]] = {
        "Given": set(),
        "When": set(),
        "Then": set(),
    }
    if not FEATURES_DIR.exists():
        print(
            f"WARNING: features directory not found: {FEATURES_DIR}",
            file=sys.stderr,
        )
        return steps

    for feature_file in sorted(FEATURES_DIR.rglob("*.feature")):
        content = feature_file.read_text(encoding="utf-8")
        last_keyword = None
        for match in STEP_PATTERN.finditer(content):
            keyword = match.group(1)
            text = match.group(2).strip()
            if keyword in ("And", "But"):
                keyword = last_keyword or "Given"
            else:
                last_keyword = keyword
            steps[keyword].add(text)

    return steps


def scan_step_definitions() -> set[str]:
    """Extract step definition patterns from Rust step files."""
    patterns: set[str] = set()
    if not STEPS_DIR.exists():
        print(
            f"WARNING: steps directory not found: {STEPS_DIR}",
            file=sys.stderr,
        )
        return patterns

    for rs_file in sorted(STEPS_DIR.rglob("*.rs")):
        content = rs_file.read_text(encoding="utf-8")
        for match in STEP_DEF_PATTERN.finditer(content):
            patterns.add(match.group(2))

    return patterns


def main() -> int:
    feature_steps = scan_feature_steps()
    step_defs = scan_step_definitions()

    all_steps: set[str] = set()
    for keyword_steps in feature_steps.values():
        all_steps.update(keyword_steps)

    total = len(all_steps)
    defined = len(step_defs)

    print(f"Feature step patterns:   {total}")
    print(f"  Given: {len(feature_steps['Given'])}")
    print(f"  When:  {len(feature_steps['When'])}")
    print(f"  Then:  {len(feature_steps['Then'])}")
    print(f"Step definitions found:  {defined}")
    print()

    # Simple matching: check if any step def pattern appears as substring
    # of a feature step (or vice versa). This is approximate.
    matched = 0
    unmatched: list[str] = []
    for step_text in sorted(all_steps):
        found = False
        for pattern in step_defs:
            try:
                if re.search(pattern, step_text):
                    found = True
                    break
            except re.error:
                if pattern in step_text or step_text in pattern:
                    found = True
                    break
        if found:
            matched += 1
        else:
            unmatched.append(step_text)

    coverage_pct = (matched / total * 100) if total > 0 else 0.0
    print(f"Matched steps: {matched}/{total} ({coverage_pct:.1f}%)")
    print()

    if unmatched:
        print(f"Unmatched steps ({len(unmatched)}):")
        for step in unmatched[:50]:
            print(f"  - {step}")
        if len(unmatched) > 50:
            print(f"  ... and {len(unmatched) - 50} more")

    return 0


if __name__ == "__main__":
    sys.exit(main())
