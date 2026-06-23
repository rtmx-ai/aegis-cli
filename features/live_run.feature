Feature: Live backlog drain
  aegis run drains a real rtmx backlog (CSV-in-git) through the built-in harness
  and a local model on loopback — closing requirements and parking failures.

  # FEAT-011
  Scenario: aegis drains a live backlog end to end
    Given a workspace
    And a mock model that emits a valid file edit
    And a live rtmx backlog with 2 closeable requirements
    When aegis drains the backlog live with the built-in harness
    Then all live requirements are closed

  # FEAT-012
  Scenario: a failing requirement parks during a live drain
    Given a workspace
    And a mock model that emits a valid file edit
    And a live rtmx backlog with one closeable and one failing requirement
    When aegis drains the backlog live with the built-in harness
    Then the failing requirement is parked
    And at least one requirement is closed
