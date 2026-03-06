# REQ-AEG-005: Dual-Ledger Auditing

## Overview
Aegis must maintain an irrefutable "Body of Evidence" for CUI handling through a dual-ledger architecture. This fulfills NIST 800-171 Audit and Accountability (3.3.x) controls by separating user-authorized actions at the edge from model executions at the cloud boundary.

## Specification
### Dual-Ledger Architecture
- **Local Edge Ledger (`~/.aegis/logs/*.jsonl`):**
  - Appends metadata-only events (JSON Lines format).
  - **No CUI payloads or prompt contents are logged locally.**
  - Events logged: `SESSION_START/END`, `CONTEXT_READ` (paths/sizes), `HITL_AUTHORIZATION` (proposed commands/diffs and human result).
- **Boundary Ledger (GCP Cloud Audit Logs):**
  - Captures `ADMIN_READ`, `DATA_READ`, and `DATA_WRITE` actions on the Vertex AI endpoint.
  - Stored in a locked Cloud Storage bucket with CMEK and a 365-day retention policy.
- **Compliance Correlation:** Support `aegis audit export --correlate` to join local `HITL_AUTHORIZATION` records with cloud-side `DATA_WRITE` events into a unified report.

## BDD Scenarios
### Scenario 1: HITL Authorization Logging
- **Given** the agent proposes a state-mutating command (`npm install`)
- **When** the human developer authorizes the command with `Y`
- **Then** the Local Edge Ledger must append a `HITL_AUTHORIZATION` entry
- **And** the entry must include the exact command, the timestamp of the proposal, and the timestamp of the human approval.

### Scenario 2: Data Minimization (No Payload Leakage)
- **Given** the agent reads a file containing sensitive code (`src/crypto_utils.ts`)
- **When** the `read_file` tool execution is logged
- **Then** the Local Edge Ledger entry for `CONTEXT_READ` must include the file path and byte size
- **And** the entry must NOT contain any of the file's contents or code.

### Scenario 3: Log Rotation Policy
- **Given** the local log file has exceeded the 90-day retention threshold
- **When** the Aegis CLI starts a new session
- **Then** the CLI must automatically prune or archive the old logs to satisfy local data minimization requirements.

## TDD Test Case Signatures
- `TestAuditLogs`: Asserts that any call to the audit logger correctly appends a well-formed JSON line to the `.jsonl` file.
- `TestLocalLedgerNoPayload`: Validates that sensitive data passed to the audit function is strictly filtered out before being persisted.
- `TestHitlAuthorizationLogging`: Ensures both the proposal and the human's response are atomically logged with millisecond precision.
- `TestLogRotation`: Mocks the system clock to verify that logs older than 90 days are deleted during the CLI startup sequence.
- `TestAuditExportCorrelation`: Verifies that the report generator correctly matches local edge timestamps with mocked GCP audit log entries.

## Acceptance Criteria
- Edge Ledger contains NO source code or prompt data.
- Audit export generates a human-readable compliance narrative.
- Local logs auto-rotate and prune after 90 days.

## Traceability
- **Parent:** GEMINI.md Section 7
- **Tests:** `pkg/audit/ledger_test.go`
