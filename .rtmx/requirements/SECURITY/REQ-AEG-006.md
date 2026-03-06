# REQ-AEG-006: Context Gating via `.aegisignore`

## Overview
To prevent accidental CUI leakage or unauthorized access to sensitive files, Aegis must strictly filter local context.

## Specification
- Evaluate every `read_file(path)` request against a blocklist defined in `.aegisignore`.
- Inherit rules from `.gitignore` by default to leverage existing developer intent.
- Mandatory blocklists: `.env`, `*.pem`, `~/.aws/credentials`, and any other secrets must be permanently denied.
- Return a structured, localized permission error to the AI agent if access is blocked.
- Support runtime modification of gating rules via the `/ignore <path>` slash command.

## BDD Scenarios
### Scenario 1: Block Restricted File Access
- **Given** the file `.env` is listed in the mandatory blocklist
- **When** the agent attempts to execute `read_file(".env")`
- **Then** the CLI must intercept the call and deny execution
- **And** return the error "File access denied by .aegisignore policy" to the agent.

### Scenario 2: Inherit from `.gitignore`
- **Given** the directory `node_modules/` is listed in `.gitignore`
- **When** the agent attempts to read `node_modules/lodash/index.js`
- **Then** the CLI must honor the `.gitignore` rule and block the request.

### Scenario 3: Runtime Ignore Rule Addition
- **Given** an active Aegis session
- **When** the user types the slash command `/ignore config/secrets.yaml`
- **And** the agent subsequently attempts to read `config/secrets.yaml`
- **Then** the CLI must deny the read request and notify the agent of the policy violation.

## TDD Test Case Signatures
- `TestAegisIgnoreParsing`: Verifies that `.aegisignore` rules (including globs and comments) are correctly parsed into the internal matcher.
- `TestGitIgnoreInheritance`: Asserts that rules from an existing `.gitignore` are correctly merged into the active context gate.
- `TestMandatoryBlocklistEnforcement`: Ensures that hardcoded secret paths are ALWAYS blocked, even if not explicitly in `.aegisignore`.
- `TestGatingErrorResponse`: Validates that the error message returned to the AI model follows the expected JSON schema and contains no sensitive data.
- `TestDynamicIgnoreUpdate`: Verifies that the internal matcher is updated in-place when a new rule is added via `/ignore`.

## Acceptance Criteria
- AI agent cannot read any file matching `.aegisignore` or `.gitignore` patterns.
- Blocked attempts are recorded in the Edge Audit Ledger for security analysis.
- `.aegisignore` updates via `/ignore` are immediate and persistent for the session.

## Traceability
- **Parent:** GEMINI.md Section 3.1
- **Tests:** `pkg/security/gate_test.go`
