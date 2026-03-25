Feature: RTMX Closed-Loop Verification
  As a defense engineer with compliance obligations
  I need the agent to link all work to RTMX requirements
  So that every code change has traceable evidence from requirement to test

  # @req REQ-RTMX-001
  Scenario: Agent reads requirements from RTMX corpus
    Given a workspace with an .rtmx/database.csv containing REQ-AUTH-001
    When I ask the agent "Implement REQ-AUTH-001"
    Then the agent should read .rtmx/database.csv
    And the agent should understand the requirement text and acceptance criteria

  # @req REQ-RTMX-002
  Scenario: Agent updates requirement status after implementation
    Given the agent has implemented code for REQ-AUTH-001
    And all linked tests pass
    When the agent updates the RTMX corpus
    Then REQ-AUTH-001 status should change from "TODO" to "COMPLETE"
    And the test_module and test_function columns should be populated
    And the completed_date should be set

  # @req REQ-RTMX-003
  Scenario: Agent refuses to close requirement with failing tests
    Given the agent has implemented code for REQ-AUTH-001
    But the linked tests are failing
    When the agent attempts to mark REQ-AUTH-001 as complete
    Then the status should remain "TODO"
    And the agent should report which tests are failing
    And the agent should attempt to fix the failing tests

  # @req REQ-RTMX-004
  Scenario: Test markers are discovered and linked
    Given a test file containing "// @req REQ-AUTH-001"
    When "rtmx health" is executed
    Then REQ-AUTH-001 should show as having linked tests
    And the test_module and test_function should match the marker location
