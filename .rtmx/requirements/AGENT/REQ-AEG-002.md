# REQ-AEG-002: Agentic REA Loop with Function Calling

## Overview
Aegis implements a strictly gated Read-Evaluate-Act (REA) Loop powered by Vertex AI Function Calling to allow the AI to interact with the local workstation securely.

## Specification
1. **Read:** Bundle user prompt, conversation history, and local tool JSON schemas.
2. **Evaluate:** Transmit payload via TLS 1.3 to Vertex AI. The model returns `functionCall` for actions.
3. **Act:** Intercept `functionCall`, route through Local Tool Schema and HITL gate.
4. **Inject:** Results (file contents, stdout, stderr, or errors) are injected back into history.

## Acceptance Criteria
- AI can successfully request and receive local file contents via `read_file`.
- AI can propose shell commands via `run_shell_command`.
- Loop terminates only when the prompt is resolved or user interrupts.

## Traceability
- **Parent:** GEMINI.md Section 5
- **Tests:** `pkg/agent/loop_test.go`
