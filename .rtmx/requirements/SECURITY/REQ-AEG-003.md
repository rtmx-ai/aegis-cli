# REQ-AEG-003: HITL Enforcement for state-mutating tools

## Overview
To satisfy CMMC Level 2 change management and authorized execution requirements, Aegis must mandate human-in-the-loop (HITL) approval for any state-mutating actions.

## Specification
- Pause the agentic loop upon intercepting `write_file` or `run_shell_command`.
- Visually alert the user in the terminal (e.g., bold yellow/red trust boundary).
- Present the exact command or file diff for approval.
- Support `Y` (Approve), `N` (Deny), and `M` (Modify).
- Log the timestamp and authorization to the local audit ledger.

## Acceptance Criteria
- No file changes or command executions occur without explicit `Y` input.
- User can modify hallucinated commands before execution using the 'Modify' escape hatch.
- Denied actions return a "permission denied" error to the AI model.

## Traceability
- **Parent:** GEMINI.md Section 5.3
- **Tests:** `pkg/security/hitl_test.go`
