Feature: Immutable Local Audit Ledger
  As a compliance officer reviewing an aegis deployment
  I need an immutable, structured log of all agent actions
  So that I can reconstruct the exact body of evidence for any session

  # @req REQ-AUDIT-001
  Scenario: Session start and stop are logged
    Given a new aegis session
    When the session starts
    Then the audit ledger should contain a SESSION_START entry
    And the entry should include session_id and timestamp
    When the session ends
    Then the audit ledger should contain a SESSION_END entry

  # @req REQ-AUDIT-001
  Scenario: Tool calls and approvals are logged
    Given an aegis session with a mock LLM
    When the agent proposes writing to "src/fix.rs"
    And the user approves the write
    Then the audit ledger should contain entries for:
      | event_type        | details                    |
      | TOOL_PROPOSED     | write_file: src/fix.rs     |
      | TOOL_APPROVED     | decision: approved         |
      | TOOL_EXECUTED     | result: success            |

  # @req REQ-AUDIT-001
  Scenario: Ledger never contains CUI content
    Given an aegis session that reads "src/classified.rs"
    And "src/classified.rs" contains sensitive source code
    When the session completes
    Then the audit ledger should contain a CONTEXT_READ entry for "src/classified.rs"
    But the audit ledger should not contain any file contents
    And the audit ledger should not contain any LLM prompts or responses

  # @req REQ-AUDIT-001
  Scenario: Ledger is append-only JSONL format
    Given an aegis session that performs multiple actions
    When I read the ledger file
    Then each line should be valid JSON
    And the file should be parseable by standard JSONL tools
    And no previous entries should be modified or deleted

  # @req REQ-AUDIT-003
  Scenario: Ledger entries link to RTMX requirements
    Given an aegis session working on REQ-HITL-001
    When the agent executes a tool call
    Then the audit ledger entry should contain req_id "REQ-HITL-001"
