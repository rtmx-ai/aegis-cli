# Long-running local agent — research synthesis & backlog

Backlog to extend aegis's ability to perform long-running tasks locally, plus codebase indexing,
expanded thinking, and borrowable features. Grounded in a survey of the 2024–2026 open-source
code-agent state of the art (four parallel research threads: long-horizon execution; reasoning /
test-time compute / memory for small models; air-gap codebase indexing; the OSS landscape).

## Core finding — the economics invert for a small local model

For gemma-4-26B-A4B (~4B active, MoE) on a memory-bandwidth-bound host (~30× slower than cloud), most
frontier test-time-compute results **do not transfer**. Techniques that spend *parallel* tokens
(self-consistency, large best-of-N, multi-agent debate) are near-fatal on CPU. Techniques that spend
*external, cheap* signal — execution/test feedback, deterministic pruning, file memory, bounded
planning — are the real wins. The literature is unanimous on one point: **small models cannot reliably
self-verify** (Huang et al ICLR'24; T1 arXiv:2504.04718; Weaver arXiv:2506.18203). A test runner *is*
the external oracle they lack — and aegis already closes on `rtmx verify`. The whole backlog leans into
that: **tests decide done; the model never self-judges.**

## What aegis already has (extend, don't rebuild)

Resumable loop with claim survival, retry→escalate, park-on-escalation, circuit breaker, per-session
run budget, backlog drain (LOOP-001..008); human-gated decomposition (PROPOSE-001..004); context-
efficiency plugin (PERF-004/005); mode-aware persona (PERSONA-001); file memory (CLAUDE.md / AGENTS.md /
skills/). The gaps are **within-task** horizon, **codebase indexing**, and **thinking tuned for the
small model** — not the across-task loop, which is solved.

## AVOID list (frontier-shaped dead ends for a 4B-active/CPU host)

- **Self-consistency & large best-of-N (N≥5)** — N× decode on a 30×-slower host; no clean voting target for diffs.
- **Multi-agent debate / critic panels** — multiple full passes; personas don't truly diverge on one small policy.
- **Intrinsic self-correction with no oracle** (classic Reflexion/Self-Refine) — often *degrades* small-model output (Huang et al).
- **Model-as-verifier gates** — sLMs are weak, poorly-calibrated verifiers. Tests decide.
- **Cloud "oracle" second-opinion model** (Amp) — a second heavy model + egress; both out for air-gap/CPU.
- **MemGPT/Letta as a runtime** — solves chat memory, wants to own the agent loop (conflicts with OpenCode), adds retrieval round-trips.
- **Auto-learned memory that rewrites CLAUDE.md/AGENTS.md** — conflicts with human-authored intent (cf. the deferred `headroom learn` decision).
- **Always-on long CoT** — below ~10B, long chains can *lower* accuracy and are pure CPU latency (Wei et al 2201.11903).
- **Parallel sub-agents** — two heavy generations contend on the memory bus; run sub-agents **sequentially**.

## Backlog

### LONGRUN — within-task long-horizon reliability
- **LONGRUN-001** — Inner run→test→fix loop: after each edit turn, run the requirement's linked test(s) and feed failures back until green or the step budget trips. *The single biggest reliability lever for a weak model; reuses rtmx test links.* (Aider, SWE-agent, OpenHands.)
- **LONGRUN-002** — Deterministic context pruning before any LLM summarization: an OpenCode plugin/hook that dedupes repeated file reads + masks stale tool observations (keep first-N + recent-N). *Cheapest horizon extension; zero extra inference.* (OpenHands condensers, Cline ContextManager.)
- **LONGRUN-003** — Persistent sub-task TODO ledger under each requirement: on-disk, seeded from the requirement, re-injected each turn, survives compaction + resume. (Claude Code Tasks/TodoWrite.)
- **LONGRUN-004** — Plan-then-act via OpenCode's existing Plan/Build agents: a read-only Plan pass writes the ledger; a Build pass executes it. Config/orchestration only. (Cline, opencode, Roo.)
- **LONGRUN-005** — Sequential sub-agent delegation for context isolation: push exploration/focused fixes into child sessions that return only a summary — **one at a time** (bandwidth). (opencode/Goose/Roo subagents.)
- **LONGRUN-006** — Grounded handoff/continuation on compaction: align OpenCode's compaction summary to carry the rtmx requirement id, todo state, and touched files so a post-compaction window resumes without drift. (opencode continuation; Amp handoff.)
- **LONGRUN-007** — Per-edit checkpoint & rollback (shadow-git): snapshot the workspace per edit so a bad mid-task change is recoverable without losing the run. (Cline/Zed checkpoints.)
- **LONGRUN-008** — Per-task step budget + soft repetition→park: extend LOOP-008 with an inner per-task step/token cap and a repeated-edit detector that parks/escalates (not hard-abort). (SWE-agent: budgets > loop-detection heuristics.)

### INDEX — air-gap codebase indexing & retrieval
- **INDEX-001** *(primary)* — Tree-sitter repo map with **personalized PageRank** ranking: parse files, extract def/ref tags via `.scm` queries, build a symbol graph, rank by relevance to the task, and elide the top-N into a fixed token budget. Zero model, zero network, cheap — the proven best fit for "the right few functions in a small window." (Aider repo-map; RepoGraph ICLR'25: code-graph context +32.8% on SWE-bench.)
- **INDEX-002** *(optional precision layer)* — SCIP precise symbol index via MCP: a local SCIP index (rust-analyzer emits SCIP natively; scip-go / scip-* per target) exposing go-to-def / find-refs / symbol-search. Compiler-accurate, **fully offline, no ML** — but a compile/dep tax and coarse re-index, so it layers *on top of* the repo map for languages where exact resolution earns its cost.
- **INDEX-003** — Grep-first retrieval doctrine + guardrail: the default retrieval path is grep/glob/LSP/repo-map, not an embeddings engine (matches context-discipline + air-gap). A Jan-2026 benchmark (arXiv:2601.08773) shows deterministic AST-derived graphs are ~20× cheaper and *more complete* than LLM-extracted ones. (Claude Code's tools-over-embeddings stance.)
- **INDEX-004** — Incremental / on-change re-indexing: keep the map fresh with **Merkle/mtime hash diffing** (re-index only changed files) + **tree-sitter `edit()`** (re-parse only changed subtrees) under a file watcher. All local, no ML, negligible CPU.
- **INDEX-005** — Context assembly & ranking: given a task, assemble a bounded bundle (repo-map slice + relevant defs + recent files), ranked to fit the small window. Persist any graph in **SQLite FTS/BM25 or an embedded store — never an embedding service**.

### THINK — reasoning / test-time compute tuned for the small model
- **THINK-001** — Calibrated reasoning budget per difficulty: reasoning length (think on/off + token budget) as a calibration param, **OFF by default** for simple requirements. (Overthinking tax; CoT hurts <10B.)
- **THINK-002** — Test-as-verifier is the only closure gate: the model never self-judges done; the linked test/compiler output decides. Guardrail. (T1, Weaver.)
- **THINK-003** — Feed test/compiler output verbatim into the edit loop: the model must *see* the failure text in-loop — the external-oracle signal. Pairs with LONGRUN-001. (T1 tool-integrated verification.)
- **THINK-004** — Self-generated test as the self-check: before marking progress, extend/write a test that captures the requirement, then run it — a local, verifiable check that beats an opinion critic for a weak model. (ReVeal arXiv:2506.11442.)
- **THINK-005** *(gated/optional)* — Tiny best-of-N (N≤3) with the **test suite as selector**, budget-gated: generate ≤3 candidate edits, keep the one that passes. Explicitly optional — unproven at 4B-active, N× decode cost. (SWE-Gym/CodeMonkeys show +8–12 pts at 30–72B, but nobody has validated the interactive-budget/CPU regime.)

### MEM — persistent, human-authored memory
- **MEM-001** — Task scratchpad file: durable notes (discoveries, decisions, running state) for a long task, re-injected; append-only, distinct from intent. (Goose memory; Claude Code notes.)
- **MEM-002** — Human-curated skill/workflow memory: extend skills/ with distilled "how we did X" routines; **induction stays manual** (no auto-learn). (Anthropic Skills; Agent Workflow Memory.)
- **MEM-003** — Project-memory assembly & precedence: assemble CLAUDE.md / AGENTS.md / skills into the prompt within the 32k budget, with clear precedence. (Zed/Amp instruction-file precedence.)
- **MEM-004** — Guardrail: no auto-learned memory that rewrites human-authored intent files. (Matches the deferred `headroom learn` decision.)

## Deferred (recorded so they aren't re-litigated)

- **Local embedding RAG over the codebase** — a fully-local embedding model (nomic-embed/bge via llama.cpp) + local store (sqlite-vec) is *possible* offline, but the research favors grep/LSP/repo-map/SCIP for a small model + air-gap + CPU cost. Re-entry trigger: only if INDEX-001/002/005 prove insufficient on a measured retrieval task.
- **LLM-extracted code graphs** — ~20× costlier, slower, and *less complete* than deterministic tree-sitter graphs (arXiv:2601.08773 — the LLM approach silently skipped 377 files). Build the graph structurally; never with the model.
- **stack-graphs (compiler-free incremental name resolution)** — the ideal design for air-gap incremental go-to-def, but GitHub archived `github/stack-graphs` on 2025-09-09. Re-entry trigger: only if we accept fork-and-maintain; otherwise INDEX-002 (SCIP / rust-analyzer) covers precise resolution.
- **Learned verifier / reward model** — adds +8–12 pts on SWE-bench at 30–72B, but is *another model to run* (bandwidth) and unproven at 4B-active/CPU. Re-entry trigger: only behind THINK-005's budget gate, with the test suite still the primary oracle.
- **ACP / harness-adapter & tool-permission governance** — borrowable framings surfaced in the landscape survey (Zed's Agent Client Protocol = "LSP for agents"; pattern-based `always_allow`/`deny`/`confirm` permissions; opencode's shadow-git `/undo` bug history argues for a *defensive* checkpoint design in LONGRUN-007). Tracked as harness/GUARD concerns, not part of this long-running backlog.
