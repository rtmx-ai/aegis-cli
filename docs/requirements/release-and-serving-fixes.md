# Release + serving fixes (REL-011, OC-027, OC-028)

Three bugs found in v1.3.0 by operator testing, fixed requirements-first.

## REQ-REL-011 — Release ships only runnable artifacts
**Bug:** the arm64 `.deb` (2 MB) shipped without the harness — `build_deb arm64` (release.sh) runs
*before* the arm64 matrix bundle is ingested, so it packages only the bare cross-binary. And bare
harness-less binaries (~6 MB) shipped alongside the full 50–67 MB bundles, and a bare aegis can't
launch the TUI.
**Fix:** build each arch's `.deb` from that arch's harness (arm64 from the ingested bundle); do **not**
publish bare harness-less binaries — every release asset launches the TUI. **Verify:**
`test::TestReleaseRunnableArtifacts`. **Deps:** REL-010.

## REQ-OC-027 — The no-model download keybind actually fires
**Bug:** OC-026's `useBindings({ bindings: [{ key: "ctrl+d", … }] })` lacked `mode:
OPENCODE_BASE_MODE`, so it registered below the prompt's `ctrl+d`=input-delete — and `ctrl+d` on an
empty prompt is app-exit. The bind never dispatched. The patch-assertion guard only checked the patch
*contained* the spawn, not that the bind was in the active mode.
**Fix:** register the bind in `OPENCODE_BASE_MODE` and use a conflict-free key (**Ctrl+G**), gated to
the no-model idle state. **Verify:** `test::TestHarnessProvisionKeybind` (the patch wires the bind in
OPENCODE_BASE_MODE). **Deps:** OC-026.

## REQ-OC-028 — Detect + offer a running Ollama
**Bug:** `ensureModelServing` only probes loopback:8080 (llama-server) + scans `~/models` for
`.gguf`; a running Ollama (`:11434`) with installed models is invisible → "no models", even though the
catalog/serving layer already understands ollama tags.
**Fix:** detect a running Ollama (probe `localhost:11434/api/tags`) and surface its installed models on
the no-model screen as a pickable serving option (operator chooses Ollama vs provisioning a fresh
model). **Verify:** `cmd/aegis::TestDetectOllama`. **Deps:** OC-022.

## REQ-OC-029 — Make a detected Ollama actually usable (context fix)
**Bug (v1.3.1, found on the M5):** the Ollama fallback (OC-028) is detected but unusable — opencode's
agent prompt (system + tool schemas) overflows Ollama's default `num_ctx` (~2048), so Ollama
**truncates** it, the model loses its instructions/tools, and flails (slow/wrong/looping). Verified:
the model fails to recall a system rule placed before an 8k-token prompt, and **`num_ctx` sent on the
OpenAI-compat `/v1` request is ignored** by Ollama — so aegis cannot fix it via opencode options.
**Fix:** aegis creates a lightweight derived model (`aegis-<model>`, `FROM <model>` + `PARAMETER
num_ctx N`, via `/api/create` — shares the base weights) and points opencode at it, so the full agent
prompt fits. **Verify:** `cmd/aegis::TestOllamaCtxModel`. **Deps:** OC-028.

## REQ-OC-031 — Verify the Ollama ctx model loads; fall back if it hangs
**Bug (v1.3.2, M5):** OC-029's derived num_ctx model hangs Ollama's load for some architectures on
Metal — confirmed with Gemma 3n (`gemma…:e4b`): the derived model hangs even on a 4-token prompt
(empty `ollama ps`), while the base loads fine. So v1.3.2 turned a slow base model into a hard hang.
**Fix:** after creating the derived model, aegis **probes** it (a 1-token generation under a short
deadline). If it doesn't answer, drop it (delete + a marker so it isn't recreated/re-probed) and use
the base model. A detected Ollama then either works or degrades to the base — never hangs.
**Verify:** `cmd/aegis::TestOllamaModelResponds`. **Deps:** OC-029.

## REQ-OC-032 — Ignore aegis's own derived models as Ollama base candidates
**Bug (v1.3.3, M5):** `detectOllama` lists ALL Ollama models including aegis's own `aegis-<model>`
derivatives (OC-029), and Ollama sorts them first (alphabetical). So `ollamaFallback` picked aegis's
own (possibly broken) derived model as the "base" — then OC-031's fall-back landed right back on it →
hang (confirmed: `models[0] = aegis-zzztest`, and the M5 had a stale v1.3.2 `aegis-gemma4-e4b`).
**Fix:** `ollamaFallback` filters out `aegis-…` names when choosing the base + the surfaced list, so
the base is always a real user model; the existence check (`ensureOllamaCtxModel`) still sees them.
**Verify:** `cmd/aegis::TestOllamaFallbackIgnoresDerived`. **Deps:** OC-031.
