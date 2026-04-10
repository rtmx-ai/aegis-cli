#!/usr/bin/env python3
"""Detect drift between BDD feature files and the RTM database.

Scans tests/features/**/*.feature for @req REQ-XXX-NNN markers and
cross-references them against .rtmx/database.csv. Reports:
  (a) REQ IDs referenced in features but absent from the database
  (b) REQ IDs in the database with BDD validation_method but no
      corresponding feature scenario

Exit code 0 if no drift detected, 1 if drift found.

Usage:
  python3 scripts/bdd-rtm-drift.py
"""

from __future__ import annotations

import csv
import re
import sys
from pathlib import Path

PROJECT_ROOT = Path(__file__).resolve().parent.parent
FEATURES_DIR = PROJECT_ROOT / "tests" / "features"
DB_PATH = PROJECT_ROOT / ".rtmx" / "database.csv"

# Match both "# @req REQ-XXX-NNN" comments and "@REQ-XXX-NNN" Gherkin tags
REQ_PATTERN = re.compile(r"@req\s+(REQ-[A-Z]+-\d+)", re.IGNORECASE)
TAG_PATTERN = re.compile(r"@(REQ-[A-Z]+-\d+)")


def scan_features() -> set[str]:
    """Extract all REQ IDs referenced in .feature files."""
    req_ids: set[str] = set()
    if not FEATURES_DIR.exists():
        print(
            f"WARNING: features directory not found: {FEATURES_DIR}",
            file=sys.stderr,
        )
        return req_ids

    for feature_file in FEATURES_DIR.rglob("*.feature"):
        content = feature_file.read_text(encoding="utf-8")
        for match in REQ_PATTERN.finditer(content):
            req_ids.add(match.group(1))
        for match in TAG_PATTERN.finditer(content):
            req_ids.add(match.group(1))

    return req_ids


def load_database() -> dict[str, dict[str, str]]:
    """Load the RTM database, keyed by req_id."""
    if not DB_PATH.exists():
        print(
            f"ERROR: database not found: {DB_PATH}",
            file=sys.stderr,
        )
        sys.exit(2)

    rows: dict[str, dict[str, str]] = {}
    with open(DB_PATH, newline="", encoding="utf-8") as f:
        reader = csv.DictReader(f)
        for row in reader:
            rows[row["req_id"]] = dict(row)
    return rows


def main() -> int:
    feature_reqs = scan_features()
    db = load_database()
    db_req_ids = set(db.keys())

    drift_found = False

    # (a) REQ IDs in features but not in database
    orphan_in_features = feature_reqs - db_req_ids
    if orphan_in_features:
        drift_found = True
        print("REQ IDs in feature files but NOT in database:")
        for req_id in sorted(orphan_in_features):
            print(f"  {req_id}")
        print()

    # (b) REQ IDs in database with BDD validation but no feature scenario
    bdd_methods = {"BDD", "BDD Test", "Functional Test"}
    missing_features: list[str] = []
    for req_id, row in sorted(db.items()):
        method = row.get("validation_method", "").strip()
        if method in bdd_methods and req_id not in feature_reqs:
            missing_features.append(req_id)

    if missing_features:
        drift_found = True
        print(
            "REQ IDs in database with BDD validation "
            "but NO feature scenario:"
        )
        for req_id in missing_features:
            print(f"  {req_id} ({db[req_id].get('validation_method', '')})")
        print()

    # Summary
    print(f"Feature REQ references: {len(feature_reqs)}")
    print(f"Database requirements:  {len(db_req_ids)}")
    if not drift_found:
        print("No BDD-RTM drift detected.")
        return 0
    else:
        print("DRIFT DETECTED -- see details above.")
        return 1


if __name__ == "__main__":
    sys.exit(main())
