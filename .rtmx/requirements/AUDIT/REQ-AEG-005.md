# REQ-AEG-005: Dual-Ledger Auditing

## Overview
Aegis must maintain an irrefutable "Body of Evidence" for CUI handling through a dual-ledger architecture (Edge and Boundary).

## Specification
- **Local Edge Ledger (`~/.aegis/logs/*.jsonl`):** Records metadata only (session metadata, paths accessed, HITL timestamps, token counts). **No CUI payloads or prompt contents are logged locally.**
- **Boundary Ledger (GCP Cloud Audit Logs):** Captures `ADMIN_READ`, `DATA_READ`, and `DATA_WRITE` actions on the Vertex AI endpoint.
- Support `aegis audit export --correlate` to match local HITL events with GCP API timestamps.

## Acceptance Criteria
- Edge Ledger contains NO source code or prompt data.
- Audit export generates a human-readable compliance narrative.
- Local logs auto-rotate and prune after 90 days.

## Traceability
- **Parent:** GEMINI.md Section 7
- **Tests:** `pkg/audit/ledger_test.go`
