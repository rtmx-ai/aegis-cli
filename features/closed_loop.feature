Feature: Closed-loop requirement completion
  As the orchestrator, aegis claims one requirement, drives the coding model on a
  loopback endpoint, verifies the result through rtmx, and closes it — the core
  thread that makes aegis-cli a local AI code-gen tool.

  # FEAT-LOOP-001
  Scenario: The loop closes a trivial requirement end-to-end
    Given a mock model endpoint on loopback
    And a backlog with one requirement that will verify successfully
    When aegis runs one iteration
    Then the requirement is closed by verify
    And the audit log records a claim and a verify
    And the model endpoint received a completion request
