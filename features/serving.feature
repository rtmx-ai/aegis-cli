Feature: Local model serving over loopback
  The inference client talks to an OpenAI-compatible endpoint on loopback only.

  # FEAT-SERVE-001
  Scenario: A chat completion round-trips over loopback
    Given a mock model endpoint on loopback
    When the client requests a chat completion
    Then the completion content is returned
    And the model endpoint received a completion request
