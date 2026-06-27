# aegis readiness

**Bottom line: the local air-gapped stack closes a real coding task end-to-end on
linux-cpu at a relaxed budget — proven, not asserted (REQ-RUNQ-004).** Interactive-latency
and the PRC-origin default (qwen3-coder) on CPU remain GPU-gated (REQ-SERVE-021).

## What is proven (RUNQ-004, 2026-06-27)

`aegis run` → OpenCode serve-drive → local model, fully air-gapped, closed a genuine coding
task (not a one-liner):

| | |
|---|---|
| Task | implement an iterative `Fib(n)` (Fib(0)=0 … Fib(20)=6765) so `go test` passes |
| Model | **gemma4-qat:32k** (Gemma-4-26B-A4B) on Ollama loopback |
| Host | linux-cpu, **no GPU** |
| Result | **closed=true** — correct implementation, `go test` passes |
| Wall-clock | **350s** (< the 900s relaxed budget) · 5 turns · ~35.9k tokens |
| Applied | RUNQ-003 step/output limits (steps=40, num_predict=8192) + SERVE-020 tuning (think:false) |

Recorded: `eval/runq-004/result.json` · guarded by `test::TestRealTaskCompletion`.

The agent read the stub, reasoned, called the edit tool, and produced:
```go
func Fib(n int) int {
	if n <= 1 {
		return n
	}
	a, b := 0, 1
	for i := 2; i <= n; i++ {
		a, b = b, a+b
	}
	return b
}
```

## Honest caveats

- **Relaxed budget, not interactive.** 350s (~6 min) for one small function is fine for the
  unattended `aegis loop`, not for interactive use. Interactive latency needs GPU
  (`SERVE-021`).
- **gemma is the CPU-capable completer; the qwen default is not (on CPU).** In the same run,
  the selected default **qwen3-coder:30b fast-failed** (returned in 244s with no correct
  edit) — consistent with SERVE-016. On CPU, point runs at `gemma4-qat:32k`; qwen3-coder is
  the agentic primary *for the GPU target*, pending `SERVE-021`.
- **Single task.** This proves the loop *can* close real work end-to-end; it is not a
  completion-rate benchmark. That is `BENCH-009` (the full intent-bench suite), still open.

## Reproduce

```bash
make build && ollama serve            # gemma4-qat:32k pulled
mkdir t && cd t && go mod init task   # add a failing stub + test (see eval/runq-004)
printf '{"endpoint":"http://127.0.0.1:11434","harness":"opencode","model_id":"gemma4-qat:32k"}' > cfg.json
aegis run --config cfg.json --workdir . --prompt "Implement Fib(n) ... use the edit tool" --timeout 900s --out t.jsonl
go test ./...                         # passes
```

## Readiness summary

| Capability | State |
|---|---|
| Build / static binary | ✅ |
| Air-gap (EGRESS=0, real OpenCode + rg) | ✅ |
| rtmx intent loop (next→drive→verify→close) | ✅ |
| Serve-drive to a real local model | ✅ |
| **Real task closed end-to-end on CPU (relaxed budget)** | ✅ RUNQ-004 |
| Origin governance gate | ✅ |
| Interactive latency / qwen on CPU | ⛔ GPU (SERVE-021) |
| Full intent-bench completion-rate | ⛔ BENCH-009 |
