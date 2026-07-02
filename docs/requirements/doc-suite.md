# aegis documentation suite on rtmx.ai

The rtmx.ai aegis presence is one page today (SITE-002). This grows it into a robust, audience-oriented
doc suite comparable to rtmx's own — organized around **Overview → Operate → Use → Secure → Reference →
Evaluate**.

## Guiding principle — reference, don't duplicate

aegis-cli already carries the canonical content in `docs/`, `skills/`, `docs/decisions/`, and its
requirement specs. Now that aegis-cli is **vendored on rtmx.ai as a submodule** (SITE-002), the site
pages are **curated, audience-oriented entry points that reference the submodule's docs** — never a copy.
Single source of truth in aegis-cli; the site provides navigation, framing, and cross-links, so the two
cannot drift. Architecture diagrams render via the site's `starlight-client-mermaid` plugin.

## Information architecture (Starlight sidebar)

```
aegis
├── Overview                (what it is, three non-negotiables, architecture diagram)
├── Getting Started         (install, quickstart, concepts)
├── Using aegis             (TUI, rtmx loop, headless, decomposition, indexing, CLI reference, config)
├── Operator & Air-Gap      (enclave deploy, air-gap setup, hardware, serving/calibration, provisioning, runbook)
├── Security & Compliance   (air-gap guarantees, ITAR, signing/SBOM, sandbox, audit, model provenance)
├── Reference               (architecture, control loop, metrics, serving internals, model corpus/bake-off)
└── Evaluate                (business case, compare, benchmarks, design decisions, roadmap)
```

## Requirements

Each requirement is verified by a **live, off-by-default check** (`AEGIS_LIVE_SITE=1`, like SITE-002) that
the section's pages are present on rtmx.ai — so the offline suite and the egress gate never make a network
call. Each depends on SITE-005 (the IA) and, transitively, SITE-002 (the page + submodule).

### SITE-005 — Doc-suite information architecture + source-of-truth wiring *(keystone)*
The Starlight sidebar declares the aegis section tree above, and a manifest (in aegis-cli) maps each site
page to the canonical submodule doc it references. No page duplicates canonical content; each links into
the submodule. Depends on SITE-002.
*References:* README, CLAUDE.md, `astro.config.mjs` sidebar. *Test:* `test::TestSiteDocsIA`

### SITE-006 — Getting Started
Overview (three non-negotiables + mermaid architecture), Installation, Quickstart (first TUI run + one
rtmx requirement to green; `aegis run` headless), Concepts (bundle-don't-fork, OpenCode + local model +
rtmx intent layer, the control loop).
*References:* README, aegis.md, CLAUDE.md §1–2. *Test:* `test::TestSiteGettingStarted`

### SITE-007 — Using aegis *(developer / agent)*
The TUI experience, the rtmx intent loop (next/claim/verify), headless runs & budgets, decomposition
(`aegis propose`), long-running tasks (ledger/memory/checkpoints/stuck-park), codebase indexing (repo map,
polyglot retrieval, degradation ladder), configuration, and a **CLI reference** (commands + flags).
*References:* operator-guide.md, runbook.md, skills/{rtmx-loop,unattended-operation,context-discipline},
docs/requirements/{aegis-tui-experience,polyglot-retrieval}.md, the `cmd/aegis` surface.
*Test:* `test::TestSiteUsingAegis`

### SITE-008 — Operator & Air-Gap deployment
Enclave deployment, air-gap setup (offline staging), hardware requirements (Ryzen/M5, memory bandwidth),
serving & calibration (bench.sh → calibration.json, resource policy), provisioning (in-TUI
download→verify→serve), day-2 runbook (unattended drain, park/breaker/budget), `verify-env`.
*References:* airgap-setup.md, enclave-deployment.md, hardware-purchase-spec.md, readiness.md,
provisioning-ux (spec), skills/{serving-calibration,airgap-hygiene,unattended-operation}.
*Test:* `test::TestSiteOperatorDocs`

### SITE-009 — Security & Compliance *(the differentiator)*
Air-gap guarantees (EGRESS=0 as a build-failing gate + how it's proven), ITAR / closed-enclave posture,
supply chain & signing (minisign verify, SBOM, the E2E security gate), sandboxed execution (bubblewrap),
audit & recordkeeping, model provenance & governance (country-of-origin policy).
*References:* release-signing.md, model-compliance.md, model-origin-governance (spec),
opencode-aegis-hardening (spec), e2e-test-suite.md, skills/airgap-hygiene.
*Test:* `test::TestSiteSecurityDocs`

### SITE-010 — Reference & Architecture
Architecture (system overview, components, module structure), the control loop (next→drive→verify→
escalate; drain/park/breaker), metrics (ACR north star + TCVR/FPVR/MTC/WCR/TCR/ESC dashboard), serving
internals, and the model corpus & bake-off (why gemma-4-26B-A4B).
*References:* CLAUDE.md, harness-serving (spec), opencode-integration (spec), skills/metrics-eval,
models.md, serve-016-bakeoff.md, model-validation.md.
*Test:* `test::TestSiteReferenceDocs`

### SITE-011 — Evaluate / Why aegis
Business case, a **Compare** page (aegis vs cloud coding agents; vs vanilla OpenCode; why local +
air-gap — parallels rtmx's guides/compare), benchmarks (intent-bench methodology + results), design
decisions surfaced from `docs/decisions/` (ADRs), and a roadmap/changelog (deferred: tree-sitter
accuracy, SCIP, two-quant).
*References:* business-case.md, intent-bench.md, docs/decisions/, readiness.md.
*Test:* `test::TestSiteEvaluateDocs`

## Non-goals
- Duplicating canonical docs on the site (reference the submodule instead).
- Auto-generating pages from the submodule wholesale — curate audience-oriented entry points.
- The demo GIF/screenshot — tracked separately (DOCS-005 / SITE-004).
