Feature: Unattended backlog drain
  Running without a human watching, aegis parks hard requirements instead of
  blocking, breaks the circuit on repeated failure, and stays inside a budget.

  # FEAT-LOOP-002
  Scenario: An unverifiable requirement is parked, not blocked
    Given a mock model endpoint on loopback
    And a backlog with one requirement that always fails verification
    And a retry budget of 2 attempts
    When aegis runs one iteration
    Then the requirement is parked

  # FEAT-LOOP-003
  Scenario: The circuit breaker halts the run after consecutive failures
    Given a mock model endpoint on loopback
    And a backlog with 5 requirements that always fail verification
    And a circuit breaker after 2 consecutive failures
    When aegis drains the backlog
    Then the circuit breaker trips

  # FEAT-LOOP-004
  Scenario: A run budget caps the number of requirements per session
    Given a mock model endpoint on loopback
    And a backlog with 3 requirements that will verify successfully
    And a run budget of 1 requirement
    When aegis drains the backlog
    Then the run stops on the budget
    And exactly 1 requirement is closed
