Feature: Secure Onboarding via aegis init
  As a defense engineer setting up aegis for the first time
  I need a guided initialization that configures my cloud backend
  So that my LLM interactions are routed through a compliant boundary

  # @req REQ-ONBOARD-001
  Scenario: Self-Service BYOC initialization with GCP
    Given the user has valid Google Cloud ADC credentials
    When the user executes "aegis init"
    And selects "Self-Service BYOC" mode
    Then the Pulumi Automation API should provision GCP Assured Workloads
    And the configuration should be saved to "~/.aegis/config.yaml"
    And the config file should have 0600 permissions

  # @req REQ-ONBOARD-003
  Scenario: Air-gapped initialization for local models
    Given no network access
    When the user executes "aegis init --local"
    Then no network calls should be made
    And the configuration should set provider to "local"
    And the configuration should set endpoint to the default Ollama address
    And "aegis chat" should work without cloud credentials

  # @req REQ-ONBOARD-002
  Scenario: Configuration contains no secrets
    Given a completed aegis init
    When I inspect "~/.aegis/config.yaml"
    Then it should contain mode, provider, region, and endpoint
    But it should not contain any API keys, tokens, or passwords
