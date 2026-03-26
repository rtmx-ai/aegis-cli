# aegis-cli

Terminal-native agentic AI pair programmer for CUI environments. Apache 2.0.

## What this is

A Rust CLI (Goose fork) that gives defense engineers a Claude Code-class AI coding experience while they control the compute, network, and data boundaries. Connects to frontier LLMs via GovCloud endpoints or runs fully offline with local models.

## Architecture

```
aegis (Rust static binary)
  |
  |-- aegis-domain        # Shared kernel: types, ports, events, errors
  |-- aegis-agent         # REA loop: read context -> LLM -> tool use -> inject
  |-- aegis-hitl          # HITL gate: blocks mutating tools until human approves
  |-- aegis-llm           # Provider abstraction: Vertex AI, Bedrock, Azure, local
  |-- aegis-tui           # ratatui single-pane TUI: chat log, input, status
  |-- aegis-audit         # Immutable JSONL ledger: metadata only, never CUI
  |-- aegis-security      # .aegisignore, sandboxing, transport, DLP
  |-- aegis-infra         # Plugin host: aegis-infra/v1 protocol over NDJSON
  |-- aegis-onboard       # aegis init state machine: 3 modes + air-gapped
  |-- aegis-cli           # Composition root: wires everything, main.rs
  |-- aegis-test-support  # Mocks, fixtures, recording helpers
  |
  |-- plugins (separate repos, invoked as subprocesses)
       |-- gcp-assured-workloads  # GCP Assured Workloads IL4/IL5 boundary
       |-- (future) aws-govcloud, azure-gov
```

**Key principle:** aegis-cli does NOT embed Pulumi or provision cloud resources directly. It invokes IaC plugins via the aegis-infra/v1 protocol. Plugins are separate binaries communicating over NDJSON stdout.

**Reference plugin:** [gcp-assured-workloads](https://github.com/rtmx-ai/gcp-assured-workloads) (TypeScript, Pulumi Automation API, 8 GCP resources)

**Plugin SDK:** [@aegis/infra-sdk](https://github.com/rtmx-ai/aegis-infra-sdk) (shared lifecycle, protocol emission, health aggregation)

## How to work on this project

### Prerequisites

- Rust stable (Homebrew or rustup)
- Git hooks: `./scripts/hooks/install.sh` (mirrors CI locally)

### Build and test

```bash
cargo build --workspace          # Build all crates
cargo test --workspace           # Run all unit + doc tests
cargo fmt --all                  # Format (required by pre-commit hook)
cargo clippy --workspace         # Lint (required by pre-commit hook)
```

### Quality gates

Pre-commit hook runs: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --lib`, `cargo test --doc`.

Pre-push hook adds: full test suite, release build check, `rtmx status` (if installed).

CI pipeline: format, lint, unit tests (Linux + Windows), integration tests, doc tests, coverage, RTMX health, binary builds (musl + MSVC).

**Nothing merges with a broken pipeline.** If CI breaks, stop all other work and fix it.

### Branch protection (REQ-TEST-005)

The `main` branch has GitHub branch protection rules enforcing:

- All CI status checks must pass before merge (see required-checks list in `.github/workflows/ci.yml`)
- At least one approving review is required
- Force pushes are disabled
- Branch deletion is disabled

Required status checks (must all be green to merge a PR):
- Format
- Lint
- Unit Tests (ubuntu-latest)
- Unit Tests (windows-latest)
- Integration Tests
- Doc Tests
- Coverage
- License & Advisory Audit (cargo-deny: licenses + RustSec advisories)
- RTMX Requirements Health
- Build (x86_64-unknown-linux-musl)
- Build (x86_64-pc-windows-msvc)
- Cross-compile Verification (x86_64-unknown-linux-musl)

### Dependency license and advisory policy (REQ-BUILD-006)

`deny.toml` at the repo root configures `cargo-deny` to:

- **Allow** only: Apache-2.0, MIT, BSD-2-Clause, BSD-3-Clause, ISC, Unicode-3.0
- **Deny** all copyleft licenses: GPL, LGPL, AGPL (any version)
- **Check** the RustSec advisory-db for known vulnerabilities on every CI run

Run locally: `cargo deny check licenses advisories`

### Adding a new feature

1. Add or update requirement in `.rtmx/database.csv`
2. Write BDD scenario in `tests/features/<category>/<name>.feature`
3. Write failing unit test with `// @req REQ-XXX-NNN` marker
4. Implement until tests pass
5. `cargo fmt --all && cargo clippy --workspace`
6. Commit (pre-commit hook validates), push (pre-push hook validates)

## Requirements

159 requirements tracked via [RTMX](https://rtmx.ai) at `.rtmx/database.csv`. 12 BDD feature files with ~450 scenarios.

Categories: BUILD (16), TUI (20), AGENT (18), HITL (8), SECURITY (10), LLM (19), INFRA (12), ONBOARD (16), AUDIT (17), RTMX (10), TEST (13).

Run `rtmx status` for current state. See `tests/features/` for executable specifications.

## Workspace layout

| Crate | Role | Key ports |
|---|---|---|
| aegis-domain | Shared kernel | ToolCall, ToolRisk, SessionId, DomainEvent |
| aegis-agent | REA loop + tools | LlmProvider, ApprovalGate, ToolExecutor, AuditLedger, SecurityFilter |
| aegis-hitl | Approval gate | HitlGate, ChannelApprovalGate, ApprovalRequest |
| aegis-llm | LLM providers | ProviderConfig, LocalProvider, create_provider() |
| aegis-tui | Terminal UI | AppState, ChatMessage, render() |
| aegis-audit | Audit ledger | JsonlLedger (JSONL append, identity binding) |
| aegis-security | Security | AegisIgnore (.aegisignore mandatory blocklist) |
| aegis-infra | Plugin host | aegis-infra/v1 protocol, NDJSON parsing, health aggregation |
| aegis-onboard | Init + config | AegisConfig, run_init(), Mode enum |
| aegis-cli | Binary | Composition root, clap CLI |
| aegis-test-support | Test infra | MockLlmProvider, MockApprovalGate, MockAuditLedger, MockToolExecutor |

## Plugin protocol (aegis-infra/v1)

Plugins are separate binaries. aegis-cli spawns them as subprocesses.

**Subcommands:** manifest, preview, up, status, destroy

**Events (NDJSON on stdout):** progress, diagnostic, check, result

**Lifecycle:** PREFLIGHT -> API_ENABLEMENT -> PROVISION -> VERIFY

**Reference plugin:** [gcp-assured-workloads](https://github.com/rtmx-ai/gcp-assured-workloads)

**Plugin SDK:** [@aegis/infra-sdk](https://github.com/rtmx-ai/aegis-infra-sdk) -- plugin authors implement 3 interfaces (CspClient, IaCEngine, HealthChecker) and call `createPluginCli(config)`.

## Target platforms

| Platform | Target | Installer |
|---|---|---|
| RHEL 8/9 | `x86_64-unknown-linux-musl` | RPM, standalone binary |
| Windows 10/11 | `x86_64-pc-windows-msvc` | MSI, standalone EXE |

Both connected (GovCloud) and air-gapped (Ollama/vLLM) modes supported.

## MVP scope

1. IaC-activated IL4/IL5 managed LLM backend (via gcp-assured-workloads plugin)
2. Functional TUI with streaming markdown, diffs, HITL approval
3. Local model support for air-gapped operation

## Style

- No emojis
- Tests before implementation
- `// @req REQ-XXX-NNN` markers on every test
- `max_width = 98` in rustfmt.toml
- Pipeline is always green
