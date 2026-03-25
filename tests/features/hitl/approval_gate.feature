Feature: Human-in-the-Loop Approval Gate
  As a defense engineer handling CUI
  I need every state-mutating agent action to require my explicit approval
  So that no unauthorized changes are made to my workstation

  # @req REQ-HITL-001
  Scenario: Agent proposes a file write and user approves
    Given the aegis agent is running with a mock LLM
    And the mock LLM will propose writing "fix applied" to "src/auth.rs"
    When I send the prompt "Fix the auth bug"
    Then the TUI should display an approval dialog for "src/auth.rs"
    And the file "src/auth.rs" should not exist yet
    When I press "y" to approve
    Then the file "src/auth.rs" should contain "fix applied"
    And the audit ledger should contain an entry with action "write_file" and approval "granted"

  # @req REQ-HITL-001
  Scenario: Agent proposes a file write and user denies
    Given the aegis agent is running with a mock LLM
    And the mock LLM will propose writing "bad code" to "src/auth.rs"
    When I send the prompt "Fix the auth bug"
    Then the TUI should display an approval dialog for "src/auth.rs"
    When I press "n" to deny
    Then the file "src/auth.rs" should not exist
    And the audit ledger should contain an entry with action "write_file" and approval "denied"
    And the agent should receive a "permission denied" tool result

  # @req REQ-HITL-001
  Scenario: Agent proposes a shell command and user edits it
    Given the aegis agent is running with a mock LLM
    And the mock LLM will propose running "rm -rf /tmp/test"
    When I send the prompt "Clean up test artifacts"
    Then the TUI should display an approval dialog for command "rm -rf /tmp/test"
    When I press "e" and change the command to "rm -rf /tmp/test/artifacts"
    Then the executed command should be "rm -rf /tmp/test/artifacts"
    And the audit ledger should contain the modified command

  # @req REQ-HITL-001
  Scenario: Read-only tools execute without approval
    Given the aegis agent is running with a mock LLM
    And the mock LLM will request reading "src/main.rs"
    When I send the prompt "Explain main.rs"
    Then the file should be read without an approval dialog
    And the agent should receive the file contents
