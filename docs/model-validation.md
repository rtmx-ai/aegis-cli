# Real-Model Validation Runbook

How to validate that a real local model (a GGUF served by `llama-server` or
Ollama on loopback) is fit to drive aegis-cli before you trust an unattended
run. This step is **out of CI** by construction — no model ships in CI — so it
is an operator procedure on the enclave host. See `docs/runbook.md` and
`docs/airgap-setup.md` for the surrounding workflow; this covers DOCS-004 and
exercises SERVE-012/013/014.

## When to run

- On a new host or after staging a new model GGUF.
- After changing quantization, context length, or the serving engine.
- Before any long unattended drain (`aegis run --max ...`).

## 1. Calibrate to the host

Tune the serving worker to this machine and record the result:

```bash
scripts/bench.sh --model /models/<your-model>.gguf
```

This writes `deploy/llama-server/calibration.json` (with the `target` field).
An uncalibrated launch is a hard error. See `skills/serving-calibration`.

## 2. Bring the endpoint up (loopback only)

Launch `llama-server`/Ollama bound to `127.0.0.1` per `deploy/llama-server/` /
`deploy/ollama/`. Confirm zero egress:

```bash
aegis verify-env
scripts/verify-airgap.sh -- aegis run --once
```

## 3. Preflight smoke (SERVE-012)

`aegis run` issues a minimal completion before claiming work; fail fast if the
model is not actually answering. To check independently, a one-shot completion
against the endpoint must return content within the timeout. A dead or wrong-port
endpoint must error here, not mid-run.

## 4. Verify model identity — digest gate (SERVE-013)

Pin the expected model so a swapped or mis-quantized GGUF cannot silently
degrade results. Set `model_id` / `model_digest` in the aegis config; at run
start the served `/v1/models` id+digest is checked and a **mismatch aborts the
run**. Record the expected digest from the GGUF you validated:

```bash
sha256sum /models/<your-model>.gguf   # the value you pin as model_digest
```

## 5. Golden-set ACR on the real model

Run the golden set through the real loop and confirm the north-star metric holds:

```bash
python3 scripts/ci-metrics.py --golden eval/golden --baseline eval/baseline.json
```

Per-call latency and token counts are surfaced by the serving client (SERVE-014)
and decompose into the prefill/decode/verify/harness breakdown (CLAUDE.md §5).

### Acceptance thresholds

- **Preflight smoke:** passes within the configured timeout.
- **Digest gate:** served id+digest match the pinned values exactly.
- **ACR:** at or above the rolling baseline in `eval/baseline.json` (the same
  ACR-regression gate CI enforces on the golden set). If ACR is below baseline,
  do not start an unattended run — re-calibrate, re-check the model, and review
  TCVR/MTC (the leading indicators) before extending the run.

## On failure

A model that fails any step is not cleared for unattended operation. Capture the
preflight/digest error or the ACR delta, fix the cause (wrong model, bad
calibration, regressed quant), and re-run this procedure from step 1.

## Running the validation as a gated test

The procedure above is executable: with a model served on loopback (e.g. Ollama),

```bash
AEGIS_REAL_ENDPOINT=http://127.0.0.1:11434 AEGIS_REAL_MODEL=phi4-mini:latest \
  go test ./test/ -run TestRealModelValidation -v
```

drives the real serving client + built-in harness against the live model
(`test/realmodel_manual_test.go`; skipped unless those env vars are set).
