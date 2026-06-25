# deploy/llama-server — production serving (llama.cpp `llama-server`)

Production inference for aegis-cli. Built **from source**, no telemetry, bound to
loopback only. One `calibration.json` (with a `target` field) drives both build
targets; `internal/serving` loads it and emits the right launch flags. An
uncalibrated launch is a hard error (`SERVE-004`).

## Files

- `calibration.json` — host-tuned config in the exact shape
  `internal/serving.Calibration` parses:
  `{ "target", "threads", "batch", "ngl", "model", "port" }`.
  Generate it with `scripts/bench.sh --model /models/your-model.gguf`. The
  committed sample targets `linux-cpu` (`ngl=0`); re-run `bench.sh` on the real
  host to replace the placeholder model path and measured thread/batch winners.

## Build from source (no telemetry, no network at runtime)

Use **`scripts/build-llama.sh`** (SERVE-017) — it pins the source via
`deploy/llama-server/LLAMA_REF`, builds target-aware (native CPU opts on
`linux-cpu`, Metal on `darwin-metal`), and is air-gapped (`-DLLAMA_CURL=OFF` strips
libcurl so the server cannot fetch models/URLs):

```bash
scripts/build-llama.sh          # -> deploy/llama-server/bin/llama-server
```

It is the codified form of (do this on a connected staging box):

```bash
# linux-cpu (Ryzen): CPU only, OpenMP threading.
cmake -B build -DGGML_NATIVE=ON -DLLAMA_CURL=OFF && cmake --build build -j --target llama-server
# darwin-metal (M5 Max): all layers on Metal.
cmake -B build -DGGML_METAL=ON -DLLAMA_CURL=OFF && cmake --build build -j --target llama-server
```

`-DLLAMA_CURL=OFF` is load-bearing. Side-load the GGUF; never let the server pull
it. Carry the built `llama-server` + the GGUF into the enclave; nothing fetches at
runtime.

## SERVE-017 — bring up + validate parity (host steps)

1. **Build:** `scripts/build-llama.sh` → `deploy/llama-server/bin/llama-server`.
2. **Model:** side-load the selected GGUF; set its path + the bake-off winner in
   `calibration.json` (`model`, `target`).
3. **Calibrate:** `scripts/bench.sh --model /models/<winner>.gguf` → fills in the
   measured `threads`/`batch`.
4. **Serve + validate parity:** launch via `internal/serving.LaunchArgs` (loopback
   `:8080`), then confirm `serving.Client.PreflightSmoke` passes and a real
   completion matches the Ollama spike — point `aegis run`'s config endpoint at
   `:8080` and re-run the keystone task. EGRESS=0 must hold.

## Launch (driven by `internal/serving.LaunchArgs`)

The orchestrator constructs the command from `calibration.json`; do not launch by
hand in production. For reference, the per-target shapes are:

- **linux-cpu** — pinned + de-prioritised, CPU only:
  `taskset -c 0-<threads-1> nice -n 5 llama-server --model <gguf> --threads <n> --batch-size <b> -ngl 0 --host 127.0.0.1 --port <port>`
- **darwin-metal** — all layers on Metal, no pinning:
  `nice -n 5 llama-server --model <gguf> --batch-size <b> -ngl 999 --host 127.0.0.1 --port <port>`

`--host 127.0.0.1` is mandatory: the endpoint is loopback-only (air-gap control).
