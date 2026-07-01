# Decision rubric: LSP vs SCIP for precise code navigation (INDEX-001-P01 / INDEX-002)

**Status:** open — rubric defined, scores partially filled, investigation pending.
**Owner input needed:** confirm weights, then run the scoped measurement to fill the `TBD` cells.

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

## Related hardening finding (independent of the outcome)

opencode's LSP downloads servers from the network when they aren't on PATH. `OPENCODE_DISABLE_LSP_DOWNLOAD`
is **not** currently set by aegis. Safe today only because LSP is off, but defense-in-depth: set it
alongside the other `OPENCODE_*` egress markers regardless of this decision.
