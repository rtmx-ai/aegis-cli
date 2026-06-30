# Responsiveness & context efficiency (PERF-001..006)

Findings (2026-06-30): llama.cpp prompt-caching HITS — a 524-token prefix re-prefills in 68 ms
(cached) vs 5,727 ms (cold). opencode's system prefix is deterministic modulo a daily date + cwd, with
no per-session nonce, so a same-day/same-project pre-warm matches the TUI session. Therefore the cold
prefill is a *one-time* cost we can move OFF the user's first prompt, and the real ceiling is window
*capacity*, not speed. Bias: eliminate first-prompt friction; keep turns ~100 ms so the agent feels
instant (stickiness); let the rtmx intent layer scope work so less must sit in-window.

## REQ-PERF-001 — Pre-splash model warmup
On launch, after the server is healthy, aegis warms the model's [system + tools] KV cache (via a
headless opencode pre-run — the prefix is deterministic per day+project) behind a "Warming <model>"
startup screen, so the operator's FIRST prompt reuses the cache and returns immediately instead of
paying the cold prefill. **Verify:** `cmd/aegis::TestWarmupPrimesCache`. **Deps:** —

## REQ-PERF-002 — Persist the KV cache across launches
Serve with llama.cpp `--prompt-cache <file>` so the warmed [system + tools] cache is saved to disk and
reloaded next launch (within the day) — turning per-launch warmup into a one-time-ever cost. **Verify:** `internal/serving::TestPromptCachePersisted`. **Deps:** PERF-001

## REQ-PERF-003 — Larger, operator-tunable served context
Raise the default served context to 32k (prefill is amortized by caching) and make it tunable via
AEGIS_CTX_SIZE, so a simple agent task does not hit "context size exceeded." **Verify:** `internal/serving::TestCtxSizeTunable`. **Deps:** —

## REQ-PERF-004 — Bounded tool-output context
Cap the size of tool outputs fed into the model's context (truncate large file reads + command output
to a head + a "truncated, N more lines" marker), so a single read cannot blow the window. **Verify:** `cmd/aegis::TestToolOutputBounded`. **Deps:** PERF-003

## REQ-PERF-005 — Strip reasoning from history
Do not re-feed a reasoning model's reasoning_content traces into later turns' context — keep only the
final content — reclaiming the window from the model's thinking. **Verify:** `internal/serving::TestReasoningStrippedFromHistory`. **Deps:** PERF-003

## REQ-PERF-006 — Context-efficiency metric
Emit a per-turn metric (prefill tokens + cache-hit ratio) so the agent loop's context efficiency is
measured and regression-gated — the prefix-stability guard that catches a harness change silently
defeating the cache. **Verify:** `internal/metrics::TestContextEfficiencyMetric`. **Deps:** PERF-001

## Mechanic update (2026-06-30, after diagnosing the latency cliff)
Path confirmed: aegis's own llama-server on :8080 serving a dedicated GGUF at 32k. The skyrocket is
opencode's COMPACTION rewriting history when 32k fills → the cached prefix changes → cold re-prefill
(measured: a turn re-prefilling ~2x its predecessor at the same token count). A reasoning model fills
the window fastest. Fix is aegis-side, no fork: opencode exposes `experimental.chat.messages.transform`
(a plugin hook). PERF-004 + PERF-005 are implemented as ONE bundled context-efficiency plugin hooking
it (drop stale `reasoning` parts; truncate oversized tool-result parts), plus tuning opencode's
compaction config (tail_turns / preserve_recent_tokens) so it preserves the cache-anchored prefix.

## REQ-PERF-007 — TUI shows the real served model
aegis's opencode.json hardcodes "local-moe" as the model name, so the TUI displays a placeholder
instead of the actual served model (the operator sees a name that doesn't match `aegis status`). Label
the model with its real id. **Verify:** `test::TestHarnessRealModelLabel`. **Deps:** —
