---
name: metrics-eval
description: Run the golden set through the real loop in a network-captured sandbox and compute/interpret the CI metrics — ACR (north star), TCVR, FPVR, MTC, WCR, TCR, ESC — and enforce the hard gates. Use this whenever you measure aegis-cli: a CI run, a model/harness bake-off, investigating a regression, or interpreting a dashboard move. Trigger it before claiming something "works", and any time you swap the model, harness, or prompt.
---

# metrics-eval

aegis-cli is measured by running it. Every CI run executes the **golden set**
(`eval/golden/`) — frozen requirements with known-correct verify outcomes — through the
*real* loop inside a network-captured sandbox, then computes the metrics. This is
intent-bench methodology turned on aegis-cli itself: not unit assertions, but "does the
whole loop autonomously close known work without egress?" Driver: `scripts/ci-metrics.py`.

## The north star and the dashboard

- **ACR — Autonomous Completion Rate** (closed-by-verify ÷ attempted, no human step) is
  the number we optimize. Everything else exists to *explain a move in ACR*, not to be
  optimized on its own.
- **Dashboard, trended every run:** TCVR (tool-call validity, ↑), FPVR (first-pass verify,
  ↑), MTC (mean turns-to-close, ↓), WCR (wall-clock per req, ↓ — CPU-bound signal), TCR
  (token cost per req, ↓), ESC (escalation rate, ↓).

## The hard gates (any failure fails the build)

1. **EGRESS = 0.** Any non-loopback packet during the run fails CI — the ITAR control as a
   test, via the same capture as `airgap-hygiene`. Non-negotiable.
2. **TRACE = 100%.** `rtmx health` passes — no orphaned requirements or tests.
3. **ACR-regression.** ACR must not fall more than the configured delta below the rolling
   baseline (`eval/baseline.json`). Catches model/harness/prompt regressions.

A green run is not "tests passed" — it is *all three gates held* and ACR within tolerance.

## Reading the numbers, especially in a bake-off

TCVR and MTC are the **leading indicators** when you swap model or harness. A model change
usually shows in TCVR (is it still emitting valid tool calls?) and MTC (is it wandering
more?) *before* it shows in ACR. So in a bake-off, don't wait for ACR to move — watch TCVR
and MTC first; a TCVR drop or MTC climb predicts the ACR hit. WCR/TCR tell you the cost of
whatever quality you got (see `context-discipline` for why they're CPU-bound), and they
decompose into per-stage timing (`serving-calibration`) so you can see *where* the cost is.

## Example

Bake-off: swap the harness from opencode to Goose, run `scripts/ci-metrics.py --golden
eval/golden --baseline eval/baseline.json`. EGRESS=0 and TRACE=100% hold. ACR is flat, but
TCVR drops 6 points and MTC climbs — Goose is emitting more malformed calls and wandering
more, paying it back in extra turns. The leading indicators caught a regression the north
star hadn't surfaced yet. Keep opencode; record why in the baseline notes.
