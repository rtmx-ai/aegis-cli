# REQ-AEG-003: HITL Enforcement for state-mutating tools

## Overview
To satisfy CMMC Level 2 change management and authorized execution requirements, Aegis must mandate human-in-the-loop (HITL) approval for any state-mutating actions.

## Specification
- Pause the agentic loop upon intercepting `write_file` or `run_shell_command`.
- Visually alert the user in the terminal using bold colors and clear iconography to signify a trust boundary.
- Present the exact command or file diff for approval.
- Support `Y` (Approve), `N` (Deny), and `M` (Modify).
- The `Modify` escape hatch drops the user into an editable text field to correct a hallucinated or incorrect command.
- Log the timestamp, user identity, and exact executed action (including modifications) to the local audit ledger.

## BDD Scenarios
### Scenario 1: Approve Command Execution
- **Given** the agent proposes the command `npm install lodash`
- **When** the HITL gate intercepts the command and prompts the user
- **And** the user selects `Y` (Approve)
- **Then** the CLI must execute the exact command `npm install lodash`
- **And** the audit ledger must record the approval and the execution timestamp.

### Scenario 2: Deny Command Execution
- **Given** the agent proposes the command `rm -rf /`
- **When** the HITL gate intercepts the command and prompts the user
- **And** the user selects `N` (Deny)
- **Then** the CLI must NOT execute the command
- **And** a "permission denied" error must be injected back into the REA loop history.

### Scenario 3: Modify Hallucinated Command
- **Given** the agent proposes the command `git commit -m "feat: useing auth"` (with a typo)
- **When** the HITL gate prompts the user
- **And** the user selects `M` (Modify)
- **And** the user corrects the command to `git commit -m "feat: using auth"`
- **Then** the CLI must execute the modified command
- **And** the audit ledger must record BOTH the proposed command and the final executed command.

## TDD Test Case Signatures
- `TestHitlPauseLoop`: Ensures that the REA loop transition to `ACTION_REQUIRED` blocks further tool execution until input is received.
- `TestHitlApprovalFlow`: Validates that selecting `Y` triggers the underlying `execa` or `fs` call with correct parameters.
- `TestHitlDenialFlow`: Asserts that selecting `N` returns a structured rejection message to the AI state machine.
- `TestHitlModifyEscapHatch`: Verifies that the interactive input buffer correctly captures user modifications and passes them to the execution layer.
- `TestHitlAuditLogging`: Ensures that the exact timestamp and final command string are atomically logged upon approval.

## Acceptance Criteria
- No file changes or command executions occur without explicit human authorization.
- User can modify hallucinated commands before execution using the 'Modify' escape hatch.
- Denied actions return a "permission denied" error to the AI model.
- HITL interactions are visually distinct and demand attention.

## Traceability
- **Parent:** GEMINI.md Section 5.3
- **Tests:** `pkg/security/hitl_test.go`
