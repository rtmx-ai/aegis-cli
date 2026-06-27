#!/usr/bin/env python3
"""intent-bench.py — METHODOLOGY DEMO (not the real intent-bench).

NOTE: This is a small control/treatment/baseline harness over toy single-function Go edits,
used to exercise the mechanics. It does NOT run the real intent-bench corpus (multi-
requirement project builds: url-shortener, task-manager) and does NOT use intent-bench's own
run process / treatments/rtmx.sh / results-PR flow. REQ-BENCH-009 is run against the real
intent-bench repo (checked out as a peer directory). See docs/requirements/intent-bench.md.

Drives the FULL experiment suite across conditions and records the comparison:

  - control   : `aegis run --no-intent` — the local model with NO rtmx intent layer
  - treatment : `aegis run`             — the local model WITH the rtmx MCP intent layer
  - baseline  : claude-code / Sonnet-class (cloud) — EGRESS, an out-of-enclave reference
                (skipped unless `claude` is on PATH; it is the only condition that leaves
                the host, by nature of being the cloud baseline)

For each (experiment, condition) it materializes a tiny real coding task, runs the agent,
scores by `go test`, and records completion / turns / tokens / wall-clock. Writes
eval/intent-bench/summary.csv + comparison.json (per-condition completion rate + a
Fisher-exact control-vs-treatment test). Std-lib only (matches serve-bakeoff / ci-metrics).

Usage:
  scripts/intent-bench.py [--endpoint URL] [--model M] \
      [--conditions control,treatment,baseline] [--runs N] [--timeout 900]
"""
import argparse
import csv
import json
import math
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import time

REPO = pathlib.Path(__file__).resolve().parent.parent
AEGIS = REPO / "bin" / "aegis"
OUT_DIR = REPO / "eval" / "intent-bench"

# The experiment set — small, real, deterministically scorable Go coding tasks. Enumerated
# here (not hard-coded to one experiment): the runner drives EVERY experiment per condition.
EXPERIMENTS = [
    {
        "name": "go-add",
        "prompt": "Edit add.go so that Add(a, b) returns a + b instead of 0. Use the edit tool, then you are done.",
        "files": {
            "go.mod": "module task\n\ngo 1.21\n",
            "add.go": "package task\n\nfunc Add(a, b int) int { return 0 }\n",
            "add_test.go": 'package task\n\nimport "testing"\n\nfunc TestAdd(t *testing.T){ if Add(2,3)!=5 { t.Fatal("want 5") } }\n',
        },
    },
    {
        "name": "go-max",
        "prompt": "Edit max.go so that Max(a, b) returns the larger of a and b (currently 0). Use the edit tool, then you are done.",
        "files": {
            "go.mod": "module task\n\ngo 1.21\n",
            "max.go": "package task\n\nfunc Max(a, b int) int { return 0 }\n",
            "max_test.go": 'package task\n\nimport "testing"\n\nfunc TestMax(t *testing.T){ if Max(2,7)!=7 || Max(9,4)!=9 { t.Fatal("wrong") } }\n',
        },
    },
    {
        "name": "go-fib",
        "prompt": "Edit fib.go so that Fib(n) returns the nth Fibonacci number (Fib(0)=0, Fib(1)=1). Use the edit tool, then you are done.",
        "files": {
            "go.mod": "module task\n\ngo 1.21\n",
            "fib.go": "package task\n\nfunc Fib(n int) int { return 0 }\n",
            "fib_test.go": 'package task\n\nimport "testing"\n\nfunc TestFib(t *testing.T){ for n,w:=range map[int]int{0:0,1:1,5:5,10:55,20:6765}{ if Fib(n)!=w { t.Fatalf("Fib(%d)=%d want %d",n,Fib(n),w) } } }\n',
        },
    },
]


def write_task(ws, exp):
    for name, content in exp["files"].items():
        (ws / name).write_text(content)


def task_passes(ws):
    env = dict(os.environ, GOFLAGS="-mod=mod")
    r = subprocess.run(["go", "test", "./..."], cwd=ws, env=env,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return r.returncode == 0


def transcript_tokens_turns(path):
    try:
        for line in path.read_text().splitlines():
            line = line.strip()
            if not line:
                continue
            d = json.loads(line)
            if d.get("type") == "result":
                u = d.get("usage", {}) or {}
                return (int(u.get("input_tokens", 0)) + int(u.get("output_tokens", 0)),
                        int(d.get("num_turns", 0)))
    except Exception:
        pass
    return 0, 0


def run_aegis(ws, exp, endpoint, model, no_intent, timeout):
    cfg = ws / "aegis.json"
    cfg.write_text(json.dumps({"endpoint": endpoint, "harness": "opencode", "model_id": model}))
    tpath = ws / "transcript.jsonl"
    argv = [str(AEGIS), "run", "--config", str(cfg), "--workdir", str(ws),
            "--prompt", exp["prompt"], "--timeout", f"{timeout}s", "--out", str(tpath)]
    if no_intent:
        argv.append("--no-intent")
    t0 = time.time()
    try:
        subprocess.run(argv, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=timeout + 60)
    except subprocess.TimeoutExpired:
        pass
    wall_ms = int((time.time() - t0) * 1000)
    tokens, turns = transcript_tokens_turns(tpath)
    return {"closed": task_passes(ws), "wall_ms": wall_ms, "tokens": tokens, "turns": turns}


def run_baseline(ws, exp, timeout):
    t0 = time.time()
    tokens = turns = 0
    try:
        r = subprocess.run(
            ["claude", "-p", exp["prompt"], "--permission-mode", "acceptEdits", "--output-format", "json"],
            cwd=ws, capture_output=True, text=True, timeout=timeout)
        try:
            j = json.loads(r.stdout)
            u = j.get("usage", {}) or {}
            tokens = int(u.get("input_tokens", 0)) + int(u.get("output_tokens", 0))
            turns = int(j.get("num_turns", 0))
        except Exception:
            pass
    except subprocess.TimeoutExpired:
        pass
    wall_ms = int((time.time() - t0) * 1000)
    return {"closed": task_passes(ws), "wall_ms": wall_ms, "tokens": tokens, "turns": turns}


def fisher_exact_two_sided(a, b, c, d):
    """Two-sided Fisher exact p for the 2x2 table [[a,b],[c,d]]. Std-lib (hypergeometric)."""
    n = a + b + c + d
    if n == 0:
        return 1.0
    row1, col1 = a + b, a + c

    def hyp(k):
        return (math.comb(row1, k) * math.comb(n - row1, col1 - k)) / math.comb(n, col1)

    p_obs = hyp(a)
    lo, hi = max(0, col1 - (n - row1)), min(row1, col1)
    return min(1.0, sum(hyp(k) for k in range(lo, hi + 1) if hyp(k) <= p_obs * 1.0000001))


def main(argv=None):
    ap = argparse.ArgumentParser(description="REQ-BENCH-009 intent-bench suite runner")
    ap.add_argument("--endpoint", default="http://127.0.0.1:11434")
    ap.add_argument("--model", default="gemma4-qat:32k", help="local model id (aegis conditions)")
    ap.add_argument("--conditions", default="control,treatment,baseline")
    ap.add_argument("--runs", type=int, default=1, help="runs per (experiment,condition)")
    ap.add_argument("--timeout", type=int, default=900)
    args = ap.parse_args(argv)

    if not AEGIS.exists():
        print(f"aegis not built: {AEGIS} (run make build)", file=sys.stderr)
        return 2
    conditions = [c.strip() for c in args.conditions.split(",") if c.strip()]
    if "baseline" in conditions and not shutil.which("claude"):
        print("intent-bench: 'claude' not on PATH — skipping the cloud baseline condition", file=sys.stderr)
        conditions = [c for c in conditions if c != "baseline"]

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    rows = []
    for cond in conditions:
        for exp in EXPERIMENTS:
            for _ in range(args.runs):
                ws = pathlib.Path(tempfile.mkdtemp(prefix=f"ib-{cond}-{exp['name']}-"))
                try:
                    write_task(ws, exp)
                    if task_passes(ws):
                        print(f"  [{cond}] {exp['name']}: PRECONDITION BAD (passes before run)", flush=True)
                        continue
                    if cond == "baseline":
                        rec = run_baseline(ws, exp, args.timeout)
                    else:
                        rec = run_aegis(ws, exp, args.endpoint, args.model, cond == "control", args.timeout)
                finally:
                    shutil.rmtree(ws, ignore_errors=True)
                rec.update({"experiment": exp["name"], "condition": cond, "attempted": 1})
                rec["closed_n"] = 1 if rec["closed"] else 0
                rows.append(rec)
                print(f"  [{cond}] {exp['name']}: closed={rec['closed']} "
                      f"wall={rec['wall_ms']/1000:.0f}s turns={rec['turns']} tokens={rec['tokens']}", flush=True)

    # summary.csv
    with open(OUT_DIR / "summary.csv", "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["experiment", "condition", "attempted", "closed", "turns", "tokens", "wall_ms"])
        for r in rows:
            w.writerow([r["experiment"], r["condition"], r["attempted"], r["closed_n"], r["turns"], r["tokens"], r["wall_ms"]])

    # per-condition completion + control-vs-treatment Fisher exact
    by_cond = {}
    for r in rows:
        c = by_cond.setdefault(r["condition"], {"attempted": 0, "closed": 0, "tokens": 0})
        c["attempted"] += 1
        c["closed"] += r["closed_n"]
        c["tokens"] += r["tokens"]
    comp = {"requirement": "REQ-BENCH-009", "model": args.model, "endpoint": args.endpoint,
            "experiments": [e["name"] for e in EXPERIMENTS], "runs_per_cell": args.runs,
            "by_condition": {c: {"completion": round(v["closed"] / v["attempted"], 3) if v["attempted"] else 0.0,
                                 "closed": v["closed"], "attempted": v["attempted"], "tokens": v["tokens"]}
                             for c, v in by_cond.items()}}
    if "control" in by_cond and "treatment" in by_cond:
        ct, tr = by_cond["control"], by_cond["treatment"]
        comp["control_vs_treatment_fisher_p"] = round(fisher_exact_two_sided(
            tr["closed"], tr["attempted"] - tr["closed"], ct["closed"], ct["attempted"] - ct["closed"]), 4)
    comp["note"] = ("Air-gapped intent-bench: aegis control (no rtmx) vs treatment (rtmx MCP) on the local "
                    "model; the baseline is the cloud claude-code reference (egress, out-of-enclave). "
                    "1 run/cell on CPU — qwen-style tool-call variance means multi-run is preferred for "
                    "tight rates (see docs); gemma is deterministic enough for a demonstration.")
    (OUT_DIR / "comparison.json").write_text(json.dumps(comp, indent=2) + "\n")
    print("\n=== by condition ===")
    for c, v in comp["by_condition"].items():
        print(f"  {c:10} completion={v['completion']} ({v['closed']}/{v['attempted']}) tokens={v['tokens']}")
    print(f"recorded -> {OUT_DIR/'summary.csv'} + comparison.json")
    return 0


if __name__ == "__main__":
    sys.exit(main())
