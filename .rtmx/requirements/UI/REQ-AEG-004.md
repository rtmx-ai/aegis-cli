# REQ-AEG-004: Reactive Terminal UI (`ink`)

## Overview
Aegis must provide a fluid, non-blocking interface using `ink` to maintain developer situational awareness in CUI environments. The UI acts as a dynamic browser canvas within the terminal, reacting to the agent's state.

## Specification
### 4-Pane Layout Architecture
- **Header Pane (Status Bar):**
  - Left: CWD and active Git branch.
  - Center: Active AI backend and enforcement boundary (e.g., `🛡️ Vertex AI: Gemini 1.5 Pro`).
  - Right: Connectivity and RTMX Sync status.
- **Context Pane (Transparency & Trust):**
  - Live list of files currently in the AI's context window.
  - **Token Utilization Meter:** Real-time visual representation of token usage (e.g., `Tokens: 14,205 / 128,000`). Color shifts (green -> yellow -> red) as limits approach.
- **Chat Log (The Scroll):**
  - Streams Markdown-formatted AI responses with syntax highlighting.
  - Collapses routine tool actions (e.g., `list_directory`) into single-line status indicators.
- **REPL Input:**
  - Multi-line input field with command history and slash command auto-completion.
  - Locks during `EVALUATING` and `TOOL_EXECUTION` states.

## BDD Scenarios
### Scenario 1: Context Pane Updates on File Read
- **Given** the Aegis agent is in an active session
- **When** the agent executes the `read_file` tool for `src/auth.ts`
- **Then** the Context Pane must immediately display `📄 src/auth.ts`
- **And** the Token Utilization Meter must increment by the token count of the file.

### Scenario 2: Token Meter Warning
- **Given** the current token usage is at 85% of the limit
- **When** the user adds a new file that pushes usage to 91%
- **Then** the Token Utilization Meter must change its visual style to a warning state (e.g., Yellow/Red).

### Scenario 3: REPL Locking during Inference
- **Given** the agent is in the `EVALUATING` state
- **When** the user attempts to type in the REPL input
- **Then** the input must be locked and a "Thinking..." spinner must be visible in the Chat Log.

## TDD Test Case Signatures
- `TestInkRender`: Verifies the base 4-pane layout mounts correctly in the terminal.
- `TestTokenMeterUpdate`: Asserts that the token meter component correctly calculates and renders percentage-based widths and colors.
- `TestContextPaneAddition`: Verifies that adding a file path to the internal state triggers a re-render of the Context Pane.
- `TestHeaderStatusDisplay`: Ensures environment variables (GCP Project, Region) are correctly mapped to the Header UI.
- `TestReplLockingState`: Validates that the input component ignores keystrokes when the global state is set to `EVALUATING`.

## Acceptance Criteria
- UI remains responsive during long-running agentic tasks.
- Token meter updates in real-time as context is added/dropped.
- `ink` components unmount/remount appropriately based on the REA state machine.

## Traceability
- **Parent:** GEMINI.md Section 6
- **Tests:** `pkg/ui/ink_test.go`
