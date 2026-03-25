Feature: Agentic REA Loop
  As a defense engineer using aegis as a pair programmer
  I need the agent to autonomously gather context and propose actions
  So that I can resolve coding tasks efficiently with AI assistance

  # @req REQ-AGENT-001
  Scenario: Agent completes a simple read-only task
    Given the aegis agent is running with a mock LLM
    And the workspace contains "src/main.rs" with a simple function
    And the mock LLM will request reading "src/main.rs" then provide an explanation
    When I send the prompt "Explain the main function"
    Then the agent should read "src/main.rs"
    And the agent should stream a text explanation
    And the loop should terminate after the explanation

  # @req REQ-AGENT-001
  Scenario: Agent performs multiple iterations to resolve a task
    Given the aegis agent is running with a mock LLM
    And the workspace contains "src/lib.rs" and "src/tests.rs"
    And the mock LLM will:
      | iteration | action                          |
      | 1         | read_file src/lib.rs            |
      | 2         | read_file src/tests.rs          |
      | 3         | write_file src/lib.rs (fix)     |
      | 4         | run_command "cargo test"        |
      | 5         | respond with "Tests fixed"      |
    When I send the prompt "Fix the failing tests"
    And I approve all proposed changes
    Then the agent should complete after 5 iterations
    And the conversation history should contain all tool calls and results

  # @req REQ-AGENT-002
  Scenario: Tool execution results are injected into conversation history
    Given the aegis agent is running with a mock LLM
    And the mock LLM will request reading "Cargo.toml"
    When the agent reads "Cargo.toml"
    Then the tool result should be appended to conversation history as a Tool role message
    And the next LLM call should include the file contents in history

  # @req REQ-AGENT-003
  Scenario: ToolShim enables tool calling for local models
    Given the aegis agent is running with a local Ollama model
    And the model does not support native function calling
    When I send the prompt "Read the README"
    Then the ToolShim should parse structured tool calls from the model output
    And the agent should execute the parsed tool call
