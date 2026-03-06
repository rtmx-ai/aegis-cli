# REQ-AEG-006: Context Gating via `.aegisignore`

## Overview
To prevent accidental CUI leakage or unauthorized access to sensitive files, Aegis must strictly filter local context.

## Specification
- Evaluate every `read_file(path)` request against a blocklist defined in `.aegisignore`.
- Inherit rules from `.gitignore` by default.
- Add mandatory blocklists for `.env`, `*.pem`, `~/.aws/credentials`, and other secrets.
- Return a localized permission error if access is blocked.

## Acceptance Criteria
- AI agent cannot read any file matching `.aegisignore` patterns.
- Blocked attempts are logged in the Edge Audit Ledger.
- `.aegisignore` can be modified on the fly using `/ignore <path>`.

## Traceability
- **Parent:** GEMINI.md Section 3.1
- **Tests:** `pkg/security/gate_test.go`
