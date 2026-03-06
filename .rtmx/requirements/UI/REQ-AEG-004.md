# REQ-AEG-004: Reactive Terminal UI (`ink`)

## Overview
Aegis must provide a fluid, non-blocking interface using `ink` to maintain developer situational awareness in CUI environments.

## Specification
- **Header Pane:** Shows CWD, Git branch, active AI backend, and VPC-SC status.
- **Context Pane:** Lists files currently in AI memory and shows a live **Token Utilization Meter**.
- **Chat Log:** Streams Markdown responses and collapses routine tool actions (e.g., `list_directory`).
- **REPL Input:** Supports multi-line input, slash commands, and terminal history (up/down).

## Acceptance Criteria
- UI remains responsive during long-running agentic tasks.
- Token meter updates in real-time as context is added/dropped.
- `ink` components unmount/remount appropriately based on the REA state machine.

## Traceability
- **Parent:** GEMINI.md Section 6
- **Tests:** `pkg/ui/ink_test.go`
