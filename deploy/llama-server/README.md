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

llama.cpp performs no phone-home, but build it deliberately and pin the source:

```bash
git clone https://github.com/ggml-org/llama.cpp     # do this on a connected staging box
cd llama.cpp

# linux-cpu (Ryzen): CPU only, no CUDA/Vulkan, OpenMP threading.
cmake -B build -DGGML_NATIVE=ON -DLLAMA_CURL=OFF
cmake --build build -j --target llama-server
# -DLLAMA_CURL=OFF is load-bearing: it strips libcurl so the server cannot fetch
# models/URLs over the network. Side-load the GGUF; never let the server pull it.

# darwin-metal (M5 Max): all layers on Metal.
cmake -B build -DGGML_METAL=ON -DLLAMA_CURL=OFF
cmake --build build -j --target llama-server
```

Carry the built `llama-server` binary + the side-loaded GGUF into the enclave.
Nothing fetches anything at runtime.

## Launch (driven by `internal/serving.LaunchArgs`)

The orchestrator constructs the command from `calibration.json`; do not launch by
hand in production. For reference, the per-target shapes are:

- **linux-cpu** — pinned + de-prioritised, CPU only:
  `taskset -c 0-<threads-1> nice -n 5 llama-server --model <gguf> --threads <n> --batch-size <b> -ngl 0 --host 127.0.0.1 --port <port>`
- **darwin-metal** — all layers on Metal, no pinning:
  `nice -n 5 llama-server --model <gguf> --batch-size <b> -ngl 999 --host 127.0.0.1 --port <port>`

`--host 127.0.0.1` is mandatory: the endpoint is loopback-only (air-gap control).
