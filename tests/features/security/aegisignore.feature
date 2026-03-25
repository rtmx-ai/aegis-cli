Feature: Context Filtering via .aegisignore
  As a defense engineer handling CUI
  I need the agent to be blocked from reading sensitive files
  So that secrets and credentials never enter the LLM context

  # @req REQ-SECURITY-001
  Scenario: Agent cannot read .env files
    Given a workspace with a ".env" file containing "SECRET_KEY=abc123"
    And an .aegisignore with default mandatory blocklist
    When the agent attempts to read ".env"
    Then it should receive a "File access denied by .aegisignore" error
    And the .env contents should never appear in the LLM payload

  # @req REQ-SECURITY-001
  Scenario: Agent cannot read PEM certificate files
    Given a workspace with "certs/server.pem" containing certificate data
    And an .aegisignore with default mandatory blocklist
    When the agent attempts to read "certs/server.pem"
    Then it should receive a "File access denied by .aegisignore" error

  # @req REQ-SECURITY-001
  Scenario: Agent can read normal source files
    Given a workspace with "src/main.rs" containing "fn main() {}"
    And an .aegisignore with default mandatory blocklist
    When the agent attempts to read "src/main.rs"
    Then it should receive the file contents successfully

  # @req REQ-SECURITY-001
  Scenario: Mandatory blocklist cannot be overridden by user
    Given an .aegisignore that explicitly allows ".env"
    When the agent attempts to read ".env"
    Then it should still receive a "File access denied" error
    Because the mandatory blocklist takes precedence
