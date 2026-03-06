# REQ-AEG-002: Agentic REA Loop with Function Calling

## Overview
Aegis implements a strictly gated Read-Evaluate-Act (REA) Loop powered by Vertex AI Function Calling to allow the AI to interact with the local workstation securely.

## Specification
1. **Read:** Bundle user prompt, conversation history, and local tool JSON schemas. Calculate token count locally to ensure compliance with egress policies.
2. **Evaluate:** Transmit payload via TLS 1.3 to Vertex AI. The model returns `functionCall` for actions or `text` for responses.
3. **Act:** Intercept `functionCall`, route through Local Tool Schema and HITL gate. The Node.js process acts as a cryptographic bouncer.
4. **Inject:** Results (file contents, stdout, stderr, or errors) are injected back into history. The loop returns to step 1 until the agent resolves the prompt.

## BDD Scenarios
### Scenario 1: Successful File Read Loop
- **Given** the user provides a prompt "Explain the logic in main.ts"
- **When** the agent evaluates the prompt and determines it needs to read `main.ts`
- **Then** it must issue a `read_file("main.ts")` function call
- **And** the CLI must execute the tool (if not blocked) and inject the contents into the history
- **And** the agent must then provide the final explanation based on the injected content.

### Scenario 2: Error Injection in Loop
- **Given** the agent attempts to call a tool that fails (e.g., `read_file` on a non-existent file)
- **When** the CLI executes the tool
- **Then** it must inject a structured error message back into the conversation history
- **And** the agent must be able to acknowledge the error and attempt an alternative strategy or report the failure to the user.

### Scenario 3: Token Limit Enforcement
- **Given** a context payload that exceeds the local CUI egress policy or Vertex AI limits
- **When** the CLI prepares the "Read" phase
- **Then** the CLI must truncate the context or reject the request before transmission
- **And** provide a warning to the user in the Context Pane.

## TDD Test Case Signatures
- `TestReaLoopStateMachine`: Verifies the state machine transitions correctly between `READ`, `EVALUATE`, `ACT`, and `INJECT`.
- `TestPayloadAssembly`: Asserts that the JSON payload sent to Vertex AI contains the correct schema for all local tools.
- `TestTokenCalculation`: Validates that the local tokenizer correctly estimates the token count of the combined prompt and history.
- `TestFunctionCallInterception`: Ensures that `functionCall` responses from the AI are correctly parsed and routed to the internal tool registry.
- `TestHistoryInjectionPersistence`: Verifies that tool outputs are correctly appended to the conversation history as `tool` role messages.

## Acceptance Criteria
- AI can successfully request and receive local file contents via `read_file`.
- AI can propose shell commands via `run_shell_command`.
- Loop terminates only when the prompt is resolved or user interrupts.
- All communications utilize FIPS-140-2 validated TLS 1.3.

## Traceability
- **Parent:** GEMINI.md Section 5
- **Tests:** `pkg/agent/loop_test.go`
