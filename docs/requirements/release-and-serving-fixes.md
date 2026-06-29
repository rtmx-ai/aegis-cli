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
