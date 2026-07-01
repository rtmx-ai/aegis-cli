# Polyglot retrieval — graceful degradation & language-set parity

New requirements arising from the LSP-vs-SCIP investigation (see
[`docs/decisions/lsp-vs-scip.md`](../decisions/lsp-vs-scip.md) and
[`mission-language-coverage.csv`](../decisions/mission-language-coverage.csv)). Two decisions from that
work drive the code here:

1. **Mission languages = rtmx's supported set** — Go, Python, Rust, JS/TS, C#, C/C++, Ruby, plus rtmx's
   universal `// rtmx:req` comment marker for everything else. aegis and rtmx share one language set.
2. **The out-of-the-box static constraint** (no downloads, statically-linked, compiled-in) means the
   precise-nav layer (LSP/SCIP) is only cleanly satisfiable for the *compiled* subset (Go, Rust, C/C++,
   + Ruby via `scip-ruby`); Python/JS/C# need a bundled runtime and are deferred. **No FFI**: tree-sitter
   for breadth runs via `wazero`/WASM (INDEX-001-P01), keeping the pure-Go static binary.

Consequence: retrieval quality is **tiered and language-dependent**, so aegis needs an explicit,
observable degradation ladder and a rule that keeps its first-class set aligned with rtmx's.

## The retrieval degradation ladder

For any file, aegis selects the best tier that is actually available, and always has a working floor:

| Tier | Mechanism | Availability | Example languages |
|---|---|---|---|
| **Precise** | LSP / SCIP (go-to-def, find-refs) | compiled langs with a bundled static server/indexer | Go, Rust, C/C++, Ruby(SCIP) |
| **Structural** | tree-sitter/WASM repo map (def/ref skeleton) | any language with an embedded grammar | all 7 + most others |
| **Grep** | ripgrep text search (INDEX-003 floor) | **every** language, always | anything |

The agent (and the metrics) must **know which tier is active** — a language served only by grep is
handled correctly, not silently treated as if precise nav were available.

## Requirements

### INDEX-007 — Retrieval degradation ladder (observable, never-fails)
For any language/file, select the best available retrieval tier (Precise → Structural → Grep) from the
declared capabilities, always falling back to Grep for an unsupported language, and surface the active
tier so degradation is explicit (no silent downgrade). Depends on INDEX-003 (the grep floor) and
INDEX-008 (the first-class set).

**Acceptance criteria**
- A language with a bundled precise server resolves to **Precise**; with only a grammar, to **Structural**;
  with neither, to **Grep**.
- An unknown/unsupported language never errors — it resolves to **Grep**.
- The selected tier is returned/renderable (observable), so callers and metrics can record it.

*Test:* `internal/index::TestRetrievalLadder`

### INDEX-008 — Language-set parity with rtmx (guardrail)
aegis's first-class retrieval language set is **equal to rtmx's supported set** by construction; anything
outside it is served by the grep floor (retrieval) and rtmx's universal comment marker (verification). A
new first-class language may only be added by adding it to **both** aegis and rtmx together — aegis never
first-classes a language rtmx cannot first-class verify (which would let the agent edit code it cannot
close a requirement against). Depends on INDEX-003.

**Acceptance criteria**
- `FirstClassLanguages()` equals rtmx's supported set (go, python, rust, javascript, typescript, c#,
  c, c++, ruby); a drift from that set fails the test.
- A language outside the set is **not** an error — it degrades to grep (ties into INDEX-007).
- LSP/SCIP coverage that exceeds rtmx's set (PHP/Elixir/Zig via LSP; Java/Kotlin/Scala via SCIP) is
  **not** first-classed unless rtmx adds it too — documented, not silently bundled.

*Test:* `internal/index::TestLanguageParity`

## Non-goals / deferred
- Bundling Node/.NET runtimes to give Python/JS/C# precise nav out-of-the-box (violates the static
  constraint) — deferred; those languages get Structural + Grep until C9 (E2E MTC/TCR) justifies it.
- First-classing languages rtmx doesn't cover (PHP, Elixir, Zig, Java, …) — parked behind the parity rule.
