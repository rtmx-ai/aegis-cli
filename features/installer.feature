Feature: Host bootstrap planning
  aegis init detects host capabilities and plans the serving target + model tier.

  # FEAT-INSTALL-001
  Scenario Outline: init plans the serving target from host capabilities
    Given a host running "<os>" with <ram> GiB of memory
    When aegis init plans the bootstrap
    Then the planned target is "<target>"

    Examples:
      | os     | ram | target       |
      | linux  | 62  | linux-cpu    |
      | darwin | 128 | darwin-metal |
