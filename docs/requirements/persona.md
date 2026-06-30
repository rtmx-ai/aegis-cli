# Agent persona & system prompt (PERSONA-001)

aegis injects operating directives into OpenCode's system prompt (the `instructions` field of the
rendered config, staged into the config-seed dir). Until now a single, terse, headless-framed directive
set was used for BOTH the interactive TUI and the headless rtmx drain — "You are in a headless session…
when the task is done and its tests pass, stop." On the interactive TUI that produced short, do-the-
minimum, low-agency replies (the operator-observed symptom).

## REQ-PERSONA-001 — Mode-aware operating directives (first persona)
The rendered config selects the directive set by session mode (cfg.Interactive): the interactive TUI
gets a proactive persona — action-biased, thorough, curious, persevering, closing with a concrete
summary — while the headless run keeps the tight, stop-when-done directives that suit an unattended,
single-requirement drain. Both directive files are staged alongside the plugin seed; the TUI launch
(Launch / HardenedEnv) marks cfg.Interactive, the headless serve/run paths leave it false.

This is aegis's FIRST system prompt + persona. It is expected to evolve toward frontier-quality
responses, developing a stronger action-based persona that combines thoroughness, precision, curiosity,
and perseverance when paired with gemma-4-26B, the rtmx intent layer, and the OpenCode harness.

**Verify:** `internal/opencode::TestModeAwareDirectives`. **Deps:** —
