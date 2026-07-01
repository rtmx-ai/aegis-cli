# Decision rubric: LSP vs SCIP for precise code navigation (INDEX-001-P01 / INDEX-002)

**Status:** open — weights confirmed; mission languages fixed (= rtmx's set); C1 + G4 scored;
FFI resolved (not needed). Remaining `TBD`: C2 (precision), C6/C7 (bundle/maintenance), C9 (measured
payoff on the E2E harness). See the "Update" section at the bottom for the reframing this produced.

The scorecard data lives in [`lsp-vs-scip-scorecard.csv`](lsp-vs-scip-scorecard.csv); this file
explains it and fixes the decision rule **before** we investigate, so the outcome isn't rationalized
after the fact.

## The question

For polyglot precise navigation (go-to-def / find-refs / symbol-search) inside the air-gapped enclave,
what fills the "depth" layer above the go/ast repo map (INDEX-001) and grep floor (INDEX-003)?

- **LSP** — opencode's built-in language servers, live. Confirmed present for 10 languages; currently
  disabled in our bundle (no `"lsp": true`). Air-gap-safe path: set `OPENCODE_DISABLE_LSP_DOWNLOAD=1`
  and stage the server binaries on PATH (the ripgrep pattern). Cost is staging + config, not new code.
- **SCIP** — precompute a compiler-grade index (`scip-go`, `scip-typescript`, …) and expose
  go-to-def/find-refs/symbol-search as MCP tools (INDEX-002). Offline + no-ML, but net-new code plus
  bundled indexer binaries.

These are **not mutually exclusive.** A valid outcome is "LSP now, SCIP later if C9 demands it."

## Structure: gates first, then weighted trade-offs

**Gates (G1-G3) are pass/fail vetoes.** A FAIL disqualifies the option regardless of weighted score —
they encode aegis's non-negotiables (zero egress, single static binary, offline/no-ML). Both options
currently PASS all three, so this is a genuine trade, not an elimination.

**Weighted criteria (C1-C9, weights sum to 100), scored 0-5** with the objective anchors in the CSV.

Weighted total (normalized to 100): `Σ(weight × score) / 5`.

**Decision rule:**
1. Any gate FAIL → disqualified.
2. Among gate-passing options, the higher weighted total is the **primary**; the other is **deferred
   pending C9** (measured payoff), not rejected.
3. C9 (golden-set MTC/TCR) is the tie-breaker and the re-open trigger — per the metric-driven policy,
   adopt the marginal option only if the numbers move.

## What the pre-filled cells already tell us

- **LSP leads on freshness (C4) and integration effort (C5)** — it's live and mostly already in the
  bundled harness.
- **SCIP leads on resource fit (C3) and auditability (C8)** — amortized query cost (important on a
  memory-bandwidth-bound CPU host) and a static inspectable index.
- **The decision hinges on the `TBD` cells** — above all **C1 (polyglot coverage, weight 20)**, then
  C2 (precision) and C9 (measured payoff). C1 needs the **enclave's actual mission-language list**,
  which is not in this (uncontrolled) repo.

## To fill before deciding (the scoped investigation)

- **C1** — obtain the mission-language set; compute coverage for LSP's 10 built-ins vs mature SCIP
  indexers.
- **C2** — labeled def/refs sample per language; score both.
- **C6/C7** — measure bundled binary sizes + count version-tracked binaries per language.
- **C9** — deferred to the E2E harness (E2E-001..004): Δ MTC / Δ TCR vs the grep-only baseline.

## Related hardening finding — RESOLVED (GUARD-004)

opencode's LSP downloads servers from the network when they aren't on PATH. Fixed in **GUARD-004**:
`OPENCODE_DISABLE_LSP_DOWNLOAD=1` is now set in the shared `airgapEnv` (hardens both the TUI launch and
the serve API), so LSP can only ever use bundled on-PATH servers, never a fetch.

---

## Update — mission languages, the out-of-the-box constraint, and FFI

### Mission languages = rtmx's supported set

aegis and rtmx **share one language set** (owner decision). rtmx's supported languages (from
`rtmx from-tests`): **Go, Python, Rust, JavaScript/TypeScript, C#, C/C++, Ruby**, plus a universal
`// rtmx:req` comment marker for anything else. Per-language server/indexer/grammar coverage +
static-linkability is in [`mission-language-coverage.csv`](mission-language-coverage.csv).

### The out-of-the-box static constraint (new gate G4) changes the shape of the answer

Owner requirement: *if aegis ships LSP or SCIP, it must work out of the box — no downloads, all
dependencies compiled in / statically-linked binaries.* Applying that to the mission set splits it cleanly:

- **Compiled languages — Go, Rust, C/C++ (and Ruby for SCIP via `scip-ruby`):** the server/indexer is a
  static, bundleable binary (gopls, rust-analyzer, clangd, scip-go, scip-clang, scip-ruby). ✅ satisfiable.
- **Runtime languages — Python, JS/TS, C#:** the servers/indexers are **Node/.NET apps** (pyright,
  typescript-language-server, roslyn, scip-python, scip-typescript, scip-dotnet). Making these "static +
  compiled-in" means **bundling an entire Node/.NET runtime** — heavy, and arguably violates the
  constraint. ❌ not cleanly satisfiable by **either** LSP or SCIP.

**Key consequence:** the out-of-the-box constraint is the binding one, and *neither* LSP nor SCIP fully
meets it — both hit the same runtime wall on Python/JS/C#. So G4 is `PARTIAL` for both. This means the
LSP-vs-SCIP precision question **only decides the compiled-language subset**; for the interpreted
languages the precise-nav layer is blocked under the constraint regardless of which we pick.

### Do we need an FFI? No.

The one option that satisfies the full out-of-the-box mandate **for all 7 languages** is the **breadth**
layer, not the precision layer: **tree-sitter (INDEX-001-P01) run via `wazero` (a pure-Go WASM runtime)
with the per-language grammar WASM blobs embedded** in the binary. That gives every language's def/ref
extraction, **compiled in, statically linked, zero downloads, and no FFI** — one Go binary.

- **FFI (CGO) is required only if** we link native tree-sitter C + grammars directly. That buys marginal
  indexing speed at the cost of CGO's cross-compile pain (esp. `darwin-metal`) and breaking the pure-Go
  static binary. Not worth it — WASM is plenty fast for indexing (off the hot path).
- SCIP `.scip` loading is pure-Go protobuf (no FFI). External servers/indexers are separate processes
  (no FFI). So **aegis needs no FFI anywhere** if tree-sitter goes the WASM route.

**Answer: no FFI.** Use `wazero` + embedded grammar WASM for INDEX-001-P01.

### Revised recommendation

1. **Breadth first, and it's the priority: INDEX-001-P01 via `wazero`/WASM (no FFI).** It's the only path
   that meets the out-of-the-box static mandate across all 7 mission languages, and it lifts the repo map
   from Go-only to polyglot. This is now the clear next INDEX build.
2. **Precision layer (LSP/SCIP) = a compiled-language bonus.** Bundle static servers/indexers for **Go,
   Rust, C/C++** (and Ruby via `scip-ruby`) where the constraint is satisfiable; for those, **LSP wins**
   (C4/C5 — live and mostly in-harness, already behind the GUARD-004 download lock). Defer precise nav for
   Python/JS/C# rather than bundle a Node/.NET runtime.
3. **Let C9 (E2E MTC/TCR) decide** whether the compiled-language precision layer is worth building at all,
   and whether Python/JS/C# precision ever justifies bundling a runtime.
