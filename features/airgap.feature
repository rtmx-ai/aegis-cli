Feature: Air-gap posture
  Every aegis component is loopback-only by construction; egress is refused.

  # FEAT-GUARD-001
  Scenario: The inference client refuses a non-loopback endpoint
    When the client is constructed for a non-loopback endpoint
    Then client construction is refused
