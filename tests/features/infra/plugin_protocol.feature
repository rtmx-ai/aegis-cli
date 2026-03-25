Feature: Infrastructure Plugin Protocol (aegis-infra/v1)
  As a defense engineer using aegis init to provision a cloud boundary
  I need aegis-cli to invoke IaC plugins via a well-defined protocol
  So that cloud provisioning is delegated to tested plugins without embedding IaC in the binary

  # ---------------------------------------------------------------------------
  # REQ-INFRA-001: aegis-infra/v1 protocol host
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-001
  Scenario: Plugin subprocess is spawned and NDJSON events are parsed
    Given a mock plugin binary that emits valid NDJSON progress and result events
    When aegis invokes the plugin with the "up" subcommand
    Then the plugin should be spawned as a child process
    And all NDJSON lines from stdout should be parsed into typed events
    And the final result event should contain vertex_endpoint and kms_key_resource_name

  # @req REQ-INFRA-001
  Scenario: Plugin input is serialized as JSON and passed via --input flag
    Given a valid aegis configuration with project_id and region
    When aegis invokes a plugin
    Then the --input flag should contain a JSON payload with project_id, region, and impact_level

  # ---------------------------------------------------------------------------
  # REQ-INFRA-002: Plugin manifest validation
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-002
  Scenario: Valid plugin manifest is accepted
    Given a plugin that responds to "manifest" with a valid aegis-infra/v1 schema
    When aegis validates the plugin
    Then the plugin should be registered successfully
    And the manifest should contain name, version, and protocol_version fields

  # @req REQ-INFRA-002
  Scenario: Plugin with incompatible protocol version is rejected
    Given a plugin that reports protocol_version "aegis-infra/v99"
    When aegis attempts to validate the plugin
    Then the plugin should be rejected with "incompatible protocol version"

  # ---------------------------------------------------------------------------
  # REQ-INFRA-003: Plugin discovery
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-003
  Scenario: Plugins are discovered from ~/.aegis/plugins/ directory
    Given executable files exist in "~/.aegis/plugins/"
    When aegis starts and scans for plugins
    Then each executable should be invoked with "manifest"
    And valid plugins should be registered

  # @req REQ-INFRA-003
  Scenario: Plugins can be registered via config.yaml
    Given config.yaml contains a plugins table with a path to a plugin binary
    When aegis starts
    Then the plugin at the configured path should be validated and registered

  # ---------------------------------------------------------------------------
  # REQ-INFRA-004: NDJSON event type parsing
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-004
  Scenario: All four event types are parsed correctly
    Given a plugin stream containing progress, diagnostic, check, and result events
    When aegis parses the stream
    Then each event should be typed as the correct variant
    And unknown event types should be logged and skipped without error

  # @req REQ-INFRA-004
  Scenario: Malformed JSON lines are handled gracefully
    Given a plugin stream containing a line that is not valid JSON
    When aegis parses the stream
    Then a warning should be logged for the malformed line
    And parsing should continue with the next line

  # ---------------------------------------------------------------------------
  # REQ-INFRA-005: Event relay to TUI
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-005
  Scenario: Progress events are displayed in the TUI during provisioning
    Given aegis is running with TUI active
    When a plugin emits progress events during "up"
    Then the TUI should display each progress message inline

  # @req REQ-INFRA-005
  Scenario: Diagnostic warnings are surfaced in the TUI status line
    Given a plugin emits a diagnostic event with severity "warning"
    When the event is relayed to the TUI
    Then the warning should appear in the status line

  # ---------------------------------------------------------------------------
  # REQ-INFRA-006 / REQ-INFRA-007: Plugin error handling and timeout
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-006
  Scenario: Non-zero plugin exit code surfaces error with stderr
    Given a plugin that exits with code 2 and stderr "provisioning failed: quota exceeded"
    When aegis invokes the plugin
    Then aegis should display "Plugin failed (exit 2): provisioning failed: quota exceeded"
    And the error should be logged to the audit ledger

  # @req REQ-INFRA-007
  Scenario: Plugin killed after timeout
    Given a plugin that hangs indefinitely
    And the plugin timeout is set to 5 seconds
    When aegis invokes the plugin
    Then the plugin should be killed after 5 seconds
    And aegis should display "Plugin timed out after 5s"

  # ---------------------------------------------------------------------------
  # REQ-INFRA-008: Config write from plugin result
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-008
  Scenario: Plugin result outputs are written to config.yaml
    Given a plugin that completes "up" with vertex_endpoint and kms_key_resource_name
    When the result event is received
    Then config.yaml should contain the outputs under infra.<plugin_name>
    And the config write should be atomic (rename-replace)

  # ---------------------------------------------------------------------------
  # REQ-INFRA-009: Teardown safety gate
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-009
  Scenario: aegis destroy requires typed project name confirmation
    Given a provisioned boundary for project "aegis-prod-001"
    When the user executes "aegis destroy"
    Then aegis should prompt "Type 'aegis-prod-001' to confirm destruction"
    And the destroy subcommand should not be sent to the plugin until confirmed

  # @req REQ-INFRA-009
  Scenario: aegis destroy aborts on incorrect confirmation
    Given a provisioned boundary for project "aegis-prod-001"
    When the user types "wrong-name"
    Then no destroy subcommand should be sent
    And the abort should be logged to the audit ledger

  # ---------------------------------------------------------------------------
  # REQ-INFRA-010: Health check aggregation
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-010
  Scenario: aegis doctor aggregates check events from all plugins
    Given two registered plugins each reporting 2 check events (all pass)
    When the user runs "aegis doctor"
    Then the TUI should display 4 individual check results
    And the overall status should be "healthy"

  # @req REQ-INFRA-010
  Scenario: One failing check causes overall degraded status
    Given a plugin reports check "kms_key_active" with status "fail"
    When the user runs "aegis doctor"
    Then the overall status should be "degraded"
    And the failing check should be highlighted

  # ---------------------------------------------------------------------------
  # REQ-INFRA-011: NIST compliance report
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-011
  Scenario: aegis verify --compliance generates control-level report
    Given health check results mapped to NIST 800-171 controls
    When the user runs "aegis verify --compliance"
    Then a report should be generated with PASS/FAIL/WARN per control ID
    And the report should be written to ~/.aegis/compliance/

  # ---------------------------------------------------------------------------
  # REQ-INFRA-012: Preview (dry-run) via plugin
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-012
  Scenario: aegis plan invokes plugin preview with no side effects
    Given a registered plugin
    When the user runs "aegis plan"
    Then the plugin should be invoked with the "preview" subcommand
    And progress events should be relayed to the TUI
    And no result should be written to config.yaml
    And no cloud resources should be created
