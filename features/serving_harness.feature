Feature: Built-in serving-backed harness
  aegis drives the local model directly to produce, sandbox, apply, and test a
  code edit — closing a requirement end to end with no external harness binary.

  # FEAT-008
  Scenario: The built-in harness closes a requirement by making its test pass
    Given a workspace
    And a mock model that emits a valid file edit
    And a backlog with one requirement that will verify successfully
    When aegis runs one iteration with the built-in harness
    Then the requirement is closed by verify
    And the edited file exists in the workspace

  # FEAT-009
  Scenario: A malformed model response is retried then the requirement closes
    Given a workspace
    And a mock model that first emits malformed output then a valid edit
    And a backlog with one requirement that will verify successfully
    When aegis runs one iteration with the built-in harness
    Then the requirement is closed by verify

  # FEAT-010
  Scenario: An out-of-workspace edit is rejected and the requirement parks
    Given a workspace
    And a mock model that emits an out-of-workspace edit
    And a backlog with one requirement that always fails verification
    When aegis runs one iteration with the built-in harness
    Then the requirement is parked
    And no file is written outside the workspace
