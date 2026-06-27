#!/usr/bin/env python3
"""serve-bakeoff.py — REQ-SERVE-016 model bake-off.

Drives `aegis run` (the air-gapped serve drive) over a set of candidate local
models on a fixed set of small coding-edit tasks, scoring each on:

  - completion : did the agent edit the file so the task's tests pass? (north star)
  - WCR (ms)   : wall-clock per task (latency budget)
  - TCR        : tokens per task (input+output, from the intent-bench transcript)

It is reproducible + documented: re-run with more `--runs` for tighter completion
rates, or a different `--models` list. Std-lib only (matches setup/ci-metrics).

Records the result to eval/bakeoff/results.json and picks the winner (highest
completion, then lowest WCR). Side-effect free except eval/bakeoff/ + the temp
workdirs. Requires: bin/aegis built, OpenCode + ripgrep staged, Ollama serving the
candidates on loopback.

Usage:
  scripts/serve-bakeoff.py [--models m1,m2,m3] [--runs N] [--timeout 240] [--endpoint URL]
"""
import argparse
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import time

REPO = pathlib.Path(__file__).resolve().parent.parent
AEGIS = REPO / "bin" / "aegis"
OUT_DIR = REPO / "eval" / "bakeoff"

# Default candidate field — diverse by size/family (a weak baseline, a small + a
# MoE gemma, and the largest contender). Override with --models.
DEFAULT_MODELS = ["phi4-mini:latest", "gemma4:e4b", "gemma4-qat:32k", "laguna-xs.2:latest"]

# Small, deterministic Go edit tasks: a stub that fails, a prompt, and a test that
# passes once the edit is right. Go (not Python) so scoring needs no extra runtime.
TASKS = [
    {
        "name": "go-add",
        "prompt": "Edit add.go so that Add(a, b) returns a + b instead of 0. Use the edit tool.",
        "files": {
            "go.mod": "module task\n\ngo 1.21\n",
            "add.go": "package task\n\nfunc Add(a, b int) int { return 0 }\n",
            "add_test.go": 'package task\n\nimport "testing"\n\nfunc TestAdd(t *testing.T){ if Add(2,3)!=5 { t.Fatal("want 5") } }\n',
        },
    },
    {
        "name": "go-max",
        "prompt": "Edit max.go so that Max(a, b) returns the larger of a and b (currently it returns 0). Use the edit tool.",
        "files": {
            "go.mod": "module task\n\ngo 1.21\n",
            "max.go": "package task\n\nfunc Max(a, b int) int { return 0 }\n",
            "max_test.go": 'package task\n\nimport "testing"\n\nfunc TestMax(t *testing.T){ if Max(2,7)!=7 || Max(9,4)!=9 { t.Fatal("wrong") } }\n',
        },
    },
]


def write_task(ws: pathlib.Path, task):
    for name, content in task["files"].items():
        (ws / name).write_text(content)


def task_passes(ws: pathlib.Path) -> bool:
    env = dict(os.environ, GOFLAGS="-mod=mod")
    r = subprocess.run(["go", "test", "./..."], cwd=ws, env=env,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return r.returncode == 0


def transcript_tokens(path: pathlib.Path) -> int:
    try:
        for line in path.read_text().splitlines():
            line = line.strip()
            if not line:
                continue
            d = json.loads(line)
            if d.get("type") == "result":
                u = d.get("usage", {})
                return int(u.get("input_tokens", 0)) + int(u.get("output_tokens", 0))
    except Exception:
        pass
    return 0


def run_one(model, cfg_path, task, timeout) -> dict:
    ws = pathlib.Path(tempfile.mkdtemp(prefix=f"bakeoff-{task['name']}-"))
    try:
        write_task(ws, task)
        if task_passes(ws):
            return {"error": "precondition: task passed before the run"}
        tpath = ws / "transcript.jsonl"
        t0 = time.time()
        r = subprocess.run(
            [str(AEGIS), "run", "--config", str(cfg_path), "--workdir", str(ws),
             "--prompt", task["prompt"], "--timeout", f"{timeout}s", "--out", str(tpath)],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            timeout=timeout + 60,
        )
        wall_ms = int((time.time() - t0) * 1000)
        passed = task_passes(ws)
        return {
            "task": task["name"],
            "closed": passed,
            "wall_clock_ms": wall_ms,
            "tokens": transcript_tokens(tpath),
            "exit": r.returncode,
        }
    except subprocess.TimeoutExpired:
        return {"task": task["name"], "closed": False, "wall_clock_ms": timeout * 1000,
                "tokens": 0, "exit": 124}
    finally:
        shutil.rmtree(ws, ignore_errors=True)


def main(argv=None):
    ap = argparse.ArgumentParser(description="REQ-SERVE-016 model bake-off")
    ap.add_argument("--models", default=",".join(DEFAULT_MODELS))
    ap.add_argument("--runs", type=int, default=1, help="runs per (model,task) for a completion rate")
    ap.add_argument("--timeout", type=int, default=240)
    ap.add_argument("--endpoint", default="http://127.0.0.1:11434")
    args = ap.parse_args(argv)

    models = [m.strip() for m in args.models.split(",") if m.strip()]
    if len(models) < 3:
        print("bake-off needs >=3 candidates", file=sys.stderr)
        return 2
    if not AEGIS.exists():
        print(f"aegis binary not built: {AEGIS} (run make build)", file=sys.stderr)
        return 2

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    results = []
    for model in models:
        cfg = OUT_DIR / f"cfg-{model.replace('/', '_').replace(':', '_')}.json"
        cfg.write_text(json.dumps({"endpoint": args.endpoint, "harness": "opencode", "model_id": model}))
        runs = []
        for task in TASKS:
            for _ in range(args.runs):
                rec = run_one(model, cfg, task, args.timeout)
                runs.append(rec)
                status = "PASS" if rec.get("closed") else "----"
                print(f"  [{model}] {rec.get('task')}: {status} "
                      f"wall={rec.get('wall_clock_ms',0)/1000:.0f}s tokens={rec.get('tokens',0)} exit={rec.get('exit')}",
                      flush=True)
        attempted = len(runs)
        closed = sum(1 for r in runs if r.get("closed"))
        agg = {
            "model": model,
            "attempted": attempted,
            "closed": closed,
            "completion": round(closed / attempted, 3) if attempted else 0.0,
            "wcr_ms": round(sum(r.get("wall_clock_ms", 0) for r in runs) / attempted) if attempted else 0,
            "tcr": round(sum(r.get("tokens", 0) for r in runs) / attempted) if attempted else 0,
            "runs": runs,
        }
        results.append(agg)
        print(f"=> {model}: completion={agg['completion']} WCR={agg['wcr_ms']}ms TCR={agg['tcr']}", flush=True)

    # Winner: highest completion, then lowest WCR, then lowest TCR. If NO candidate
    # completed any task within the budget, there is no winner-by-completion — say so
    # rather than crowning the fastest-failing model (a budget too tight for the field).
    ranked = sorted(results, key=lambda r: (-r["completion"], r["wcr_ms"], r["tcr"]))
    best = ranked[0]
    if best["completion"] == 0.0:
        winner = ""
        winner_basis = f"no candidate completed any task within the {args.timeout}s budget"
    else:
        winner = best["model"]
        winner_basis = "best completion (PASS fraction), then lowest WCR, then lowest TCR"
    out = {
        "requirement": "REQ-SERVE-016",
        "endpoint": args.endpoint,
        "budget_s": args.timeout,
        "runs_per_cell": args.runs,
        "tasks": [t["name"] for t in TASKS],
        "candidates": [{k: v for k, v in r.items() if k != "runs"} for r in results],
        "detail": results,
        "winner": winner,
        "winner_basis": winner_basis,
        "scoring": "completion (PASS fraction) primary; WCR (ms) then TCR (tokens) tiebreak",
    }
    (OUT_DIR / "results.json").write_text(json.dumps(out, indent=2) + "\n")
    print(f"\nWINNER: {winner['model']} (completion={winner['completion']}, WCR={winner['wcr_ms']}ms, TCR={winner['tcr']})")
    print(f"recorded -> {OUT_DIR / 'results.json'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
