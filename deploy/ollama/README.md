# deploy/ollama — spike serving (localhost-bound)

Ollama is the **fast-iteration spike** serving layer only (`CLAUDE.md` §2).
Production serving is llama.cpp `llama-server` (see `../llama-server`). Use Ollama
to side-load a GGUF and iterate quickly; promote the bake-off winner to
`llama-server` for the closed run.

## Hardening (air-gap posture)

`ollama.env` pins the offline-safe environment:

- `OLLAMA_HOST=127.0.0.1:11434` — API bound to **loopback only**, never
  `0.0.0.0` (that would expose the endpoint off-box).
- Update checks / phone-home **off** — Ollama probes for updates on launch; the
  durable control is the default-deny egress firewall (`../firewall`) plus an
  offline host. Keep the box disconnected; never let it pull a model at runtime.
- Single loaded model, bounded keep-alive — predictable memory on a
  bandwidth-bound host.

## Usage

```bash
set -a; . deploy/ollama/ollama.env; set +a
ollama serve &                       # binds 127.0.0.1:11434
ollama create mymodel -f Modelfile   # side-load a local GGUF (no pull)
```

Point the harness at `http://127.0.0.1:11434` (OpenAI-compatible). The same
loopback-only guarantee the app enforces applies here: any non-loopback bind is a
misconfiguration the egress gate is designed to catch.
