---
name: serving-calibration
description: Tune the local inference worker to the host, govern compute contention, and profile per-stage timing — calibrate (don't guess), place (don't schedule), separate generate from verify in time. Use this whenever you touch serving: launching the endpoint, setting thread/batch/offload, diagnosing slow WCR, or bringing up a new target. Trigger it before any launch (uncalibrated is a hard error) and any time two heavy stages might run at once.
---

# serving-calibration

Almost all compute lives in the inference worker (`CLAUDE.md` §2); aegis-cli is I/O-bound
and uses negligible CPU. So "optimal resource use" is never about the orchestrator — it's
about tuning `llama-server` to *this host* and keeping bandwidth-heavy stages out of each
other's way. The binding constraint is **memory bandwidth** on both targets — the DDR4 bus
on the Ryzen, the 614 GB/s unified memory on the Mac — so two bandwidth-heavy stages at
once each get slower. Four principles follow, all OS primitives + config, never a custom
scheduler.

## The four principles

- **Calibrate, don't guess.** `scripts/bench.sh` auto-detects the target and sweeps the
  right knobs once — thread/batch with `-ngl 0` on `linux-cpu`, batch with all layers on
  Metal (`-ngl 999`) on `darwin-metal` — and writes the winner (with a `target` field) to
  `deploy/llama-server/calibration.json`. `internal/serving` loads it at launch and emits
  the right flags. **Uncalibrated launch is a hard error** — no guessed defaults ship.
- **Place, don't schedule.** On `linux-cpu`, pin inference to physical cores (`taskset`)
  and de-prioritise co-located workers (`nice`). On `darwin-metal` there is no `taskset`
  and pinning isn't the lever — the GPU does the work; `nice` still applies. OS primitives,
  not a scheduler you wrote.
- **Separate in time.** The loop avoids overlapping generate and verify on the bus — phase
  separation beats core-partitioning here, on both targets (`LOOP-004`). Don't run a
  bandwidth-heavy verify while decode is still saturating memory.
- **Profile via metrics, not a separate tool.** WCR/TCR decompose into prefill / decode /
  verify / harness-overhead, emitted from `internal/metrics`. That breakdown *is* the
  profiler — read it instead of bolting on a profiling tool.

## Dual target, one config

One `calibration.json` with a `target` field drives both hosts; the only difference is at
launch — CPU + `taskset` (`-ngl 0`) on the Ryzen, all-layers-on-Metal, no pinning
(`-ngl 999`) on the M5 Max. Build and validate on `linux-cpu` first; the Mac path is wired
and waiting (`SERVE-005`). Don't fork the launch logic per target — branch on the field.

## Example

New Ryzen host: `scripts/bench.sh --model /models/your-model.gguf` sweeps thread/batch at
`-ngl 0`, picks the winner, writes `calibration.json` with `target: linux-cpu`. At launch,
`internal/serving` loads it, pins inference with `taskset` and `nice`s the harness. WCR
looks high — read the per-stage metrics: decode is fine but verify time spikes overlapping
decode. Enforce phase separation (`LOOP-004`); WCR drops. No profiler installed, no
scheduler written, no guessed flags — calibrated, placed, separated.
