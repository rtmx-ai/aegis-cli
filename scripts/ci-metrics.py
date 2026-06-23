#!/usr/bin/env python3
"""ci-metrics.py — compute the CLAUDE.md §5 dashboard metrics over the golden set.

Replays the frozen golden-set fixtures (eval/golden/*.json) and computes the same
aggregate metrics the Go collector (internal/metrics) emits at runtime:

    ACR  (north star)  TCVR  FPVR  MTC  WCR  TCR  ESC  + per-stage timing

Then compares measured ACR to the rolling baseline (eval/baseline.json) and exits
NON-ZERO if ACR has regressed more than the configured delta below baseline — the
ACR-regression hard gate. EGRESS=0 and TRACE=100% are the other two hard gates,
enforced by scripts/verify-airgap.sh and `rtmx health` respectively (see Makefile).

Python 3 standard library only — no third-party deps, no network, ITAR-clean.

Usage:
    python3 scripts/ci-metrics.py --golden eval/golden --baseline eval/baseline.json
"""

import argparse
import json
import sys
from pathlib import Path


def load_fixtures(golden_dir: Path):
    """Load every *.json golden fixture (ignoring README and dotfiles)."""
    fixtures = []
    for p in sorted(golden_dir.glob("*.json")):
        with p.open(encoding="utf-8") as fh:
            data = json.load(fh)
        data["_path"] = str(p)
        fixtures.append(data)
    if not fixtures:
        raise SystemExit(f"ci-metrics: no golden fixtures found in {golden_dir}")
    return fixtures


def compute(fixtures):
    """Aggregate metrics over fixtures, mirroring internal/metrics.Collector.Report."""
    attempted = len(fixtures)
    closed = escalated = first_pass = 0
    closed_turns = 0
    total_tool_calls = total_valid_tool_calls = 0
    total_tokens = 0
    total_wall_ms = 0
    stages = {"prefill": 0, "decode": 0, "verify": 0, "harness_overhead": 0}

    for a in fixtures:
        if a.get("closed"):
            closed += 1
            closed_turns += int(a.get("turns", 0))
            if a.get("first_pass"):
                first_pass += 1
        if a.get("escalated"):
            escalated += 1
        total_tool_calls += int(a.get("tool_calls", 0))
        total_valid_tool_calls += int(a.get("valid_tool_calls", 0))
        total_tokens += int(a.get("tokens", 0))
        total_wall_ms += int(a.get("wall_clock_ms", 0))
        s = a.get("stages_ms", {})
        for k in stages:
            stages[k] += int(s.get(k, 0))

    report = {
        "attempted": attempted,
        "closed": closed,
        "escalated": escalated,
        "acr": closed / attempted,
        "esc": escalated / attempted,
        "wcr_ms": total_wall_ms / attempted,
        "tcr": total_tokens / attempted,
        "tcvr": (total_valid_tool_calls / total_tool_calls) if total_tool_calls else 0.0,
        "fpvr": (first_pass / closed) if closed else 0.0,
        "mtc": (closed_turns / closed) if closed else 0.0,
        "stages_ms": stages,
    }
    return report


def fmt(report):
    """Human-readable dashboard, mirroring §5's table."""
    lines = [
        "==================== aegis-cli golden-set metrics ====================",
        f"  attempted          : {report['attempted']}",
        f"  closed (no human)  : {report['closed']}",
        f"  escalated          : {report['escalated']}",
        "  --------------------------------------------------------------",
        f"  ACR  (north star ↑): {report['acr']:.3f}",
        f"  TCVR (validity ↑)  : {report['tcvr']:.3f}",
        f"  FPVR (first-pass ↑): {report['fpvr']:.3f}",
        f"  MTC  (turns ↓)     : {report['mtc']:.2f}",
        f"  WCR  (ms ↓)        : {report['wcr_ms']:.0f}",
        f"  TCR  (tokens ↓)    : {report['tcr']:.0f}",
        f"  ESC  (rate ↓)      : {report['esc']:.3f}",
        "  --- per-stage timing (ms; the profiler) ----------------------",
        f"  prefill={report['stages_ms']['prefill']} decode={report['stages_ms']['decode']} "
        f"verify={report['stages_ms']['verify']} harness={report['stages_ms']['harness_overhead']}",
        "======================================================================",
    ]
    return "\n".join(lines)


def main(argv=None):
    ap = argparse.ArgumentParser(description="Compute golden-set metrics and enforce the ACR-regression gate.")
    ap.add_argument("--golden", required=True, type=Path, help="golden-set directory (eval/golden)")
    ap.add_argument("--baseline", required=True, type=Path, help="baseline JSON (eval/baseline.json)")
    ap.add_argument("--json", action="store_true", help="emit the report as JSON instead of a table")
    args = ap.parse_args(argv)

    fixtures = load_fixtures(args.golden)
    report = compute(fixtures)

    with args.baseline.open(encoding="utf-8") as fh:
        baseline = json.load(fh)
    baseline_acr = float(baseline.get("baseline_acr", 0.0))
    delta = float(baseline.get("acr_regression_delta", 0.0))

    if args.json:
        print(json.dumps(report, indent=2))
    else:
        print(fmt(report))

    floor = baseline_acr - delta
    print(f"\nACR gate: measured={report['acr']:.3f} baseline={baseline_acr:.3f} "
          f"delta={delta:.3f} floor={floor:.3f}")
    if report["acr"] < floor:
        print("ACR-REGRESSION: FAIL — ACR fell below the baseline floor. (hard gate)", file=sys.stderr)
        return 1
    print("ACR-regression: PASS (within tolerance).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
