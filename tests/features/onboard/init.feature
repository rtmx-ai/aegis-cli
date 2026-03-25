Feature: Secure Onboarding via aegis init
  As a defense engineer setting up aegis for the first time or updating an existing install
  I need a guided initialization that configures my cloud backend, proxy, and credentials
  So that my LLM interactions are routed through a compliant boundary with no secrets in config

  # ---------------------------------------------------------------------------
  # REQ-ONBOARD-001: State machine with three deployment modes
  # ---------------------------------------------------------------------------

  # @req REQ-ONBOARD-001
  Scenario: Self-Service BYOC initialization with GCP
    Given the user has valid Google Cloud ADC credentials
    When the user executes "aegis init"
    And selects "Self-Service BYOC" mode
    Then the Pulumi Automation API should provision GCP Assured Workloads
    And the configuration should be saved to "~/.aegis/config.yaml"
    And the config file should have 0600 permissions

  # @req REQ-ONBOARD-001
  Scenario: Environment probe detects missing cloud credentials and halts
    Given no GCP credentials exist on the workstation
    When the user executes "aegis init"
    And selects "Self-Service BYOC" mode
    Then the environment probe stage should detect missing credentials
    And display an error directing the user to run "gcloud auth application-default login"
    And no infrastructure should be provisioned
    And no config file should be written

  # ---------------------------------------------------------------------------
  # REQ-ONBOARD-002: Config artifact at ~/.aegis/config.yaml with 0600 perms
  # ---------------------------------------------------------------------------

  # @req REQ-ONBOARD-002
  Scenario: Configuration contains no secrets
    Given a completed "aegis init"
    When I inspect "~/.aegis/config.yaml"
    Then it should contain fields: mode, provider, region, endpoint, schema_version
    But it should not contain any API keys, tokens, or passwords
    And the file permissions should be 0600

  # @req REQ-ONBOARD-002
  Scenario: Config file is rejected if permissions are too permissive
    Given "~/.aegis/config.yaml" exists with permissions 0644
    When the user executes any "aegis" command
    Then aegis should exit with code 78
    And display "Config file permissions must be 0600, found 0644"
    And display a remediation hint to run "chmod 0600 ~/.aegis/config.yaml"

  # ---------------------------------------------------------------------------
  # REQ-ONBOARD-003: Air-gapped initialization for local models
  # ---------------------------------------------------------------------------

  # @req REQ-ONBOARD-003
  Scenario: Air-gapped initialization for local models
    Given no network access is available
    When the user executes "aegis init --local"
    Then no network calls should be made
    And the configuration should set mode to "local"
    And the configuration should set endpoint to the default Ollama address "http://localhost:11434"
    And "aegis chat" should work without cloud credentials

  # @req REQ-ONBOARD-003
  Scenario: Air-gapped init with custom local endpoint
    Given no network access is available
    When the user executes "aegis init --local --endpoint http://192.168.1.50:11434"
    Then the configuration should set endpoint to "http://192.168.1.50:11434"
    And no network calls should be made during initialization

  # ---------------------------------------------------------------------------
  # REQ-ONBOARD-004 / REQ-ONBOARD-005: Re-initialization
  # ---------------------------------------------------------------------------

  # @req REQ-ONBOARD-004
  Scenario: Re-initialization detects existing config and offers update menu
    Given "~/.aegis/config.yaml" already exists with mode "self-service-byoc"
    When the user executes "aegis init"
    Then aegis should display "Existing configuration detected"
    And present options: "Update credentials", "Change mode", "Rotate infra secrets", "Full reset"
    And not immediately overwrite the existing config file

  # @req REQ-ONBOARD-004
  Scenario: Re-initialization credential update preserves infrastructure binding
    Given "~/.aegis/config.yaml" already exists with mode "self-service-byoc"
    When the user executes "aegis init"
    And selects "Update credentials"
    Then only the Credential Negotiation stage should re-run
    And the existing IaC state should be preserved
    And the config endpoint and region should remain unchanged

  # @req REQ-ONBOARD-004
  Scenario: Full reset during re-initialization reprovisioning from scratch
    Given "~/.aegis/config.yaml" already exists with mode "self-service-byoc"
    When the user executes "aegis init"
    And selects "Full reset"
    And confirms the destructive action
    Then the full state machine should re-run from Environment Probe
    And new infrastructure should be provisioned
    And a new config should be written

  # @req REQ-ONBOARD-005
  Scenario: Audit ledger is preserved during re-initialization
    Given "~/.aegis/logs/" contains existing audit entries
    When the user executes "aegis init" and completes any update path
    Then "~/.aegis/logs/" should still contain all prior audit entries
    And no log files should be deleted or truncated

  # @req REQ-ONBOARD-005
  Scenario: Full reset archives audit ledger rather than deleting it
    Given "~/.aegis/logs/" contains existing audit entries
    When the user executes "aegis init" and selects "Full reset"
    Then the existing audit entries should be moved to "~/.aegis/logs/archive/"
    And the archive directory name should include an ISO-8601 timestamp
    And a new empty ledger session should begin after reset

  # ---------------------------------------------------------------------------
  # REQ-ONBOARD-006: Credential rotation
  # ---------------------------------------------------------------------------

  # @req REQ-ONBOARD-006
  Scenario: Credential rotation updates references without reprovisioning
    Given a valid "~/.aegis/config.yaml" with mode "self-service-byoc"
    When the user executes "aegis rotate-credentials"
    Then only the Credential Negotiation stage should run
    And the infrastructure should not be modified
    And the new credentials should be validated before overwriting old references
    And a rotation event should be recorded in the audit ledger

  # @req REQ-ONBOARD-006
  Scenario: Credential rotation fails fast if new credentials are invalid
    Given a valid "~/.aegis/config.yaml"
    And the user provides invalid credentials during rotation
    When the user executes "aegis rotate-credentials"
    Then aegis should reject the new credentials before overwriting old ones
    And the existing config should remain unchanged
    And an error should describe why the new credentials failed validation

  # ---------------------------------------------------------------------------
  # REQ-ONBOARD-007: Proxy and CA bundle configuration
  # ---------------------------------------------------------------------------

  # @req REQ-ONBOARD-007
  Scenario: Proxy URL is configured via aegis init prompt
    Given a corporate network requiring proxy "https://proxy.corp.example:8080"
    When the user executes "aegis init"
    And enters "https://proxy.corp.example:8080" at the proxy URL prompt
    Then "~/.aegis/config.yaml" should contain "proxy_url: https://proxy.corp.example:8080"
    And all subsequent outbound connections should route through that proxy

  # @req REQ-ONBOARD-007
  Scenario: Custom CA bundle is accepted for TLS-inspecting middleboxes
    Given a CA bundle at "/etc/ssl/corp-ca-bundle.pem"
    When the user executes "aegis init"
    And specifies "/etc/ssl/corp-ca-bundle.pem" as the CA bundle path
    Then "~/.aegis/config.yaml" should contain "ca_bundle_path: /etc/ssl/corp-ca-bundle.pem"
    And the CA bundle should be loaded into the TLS stack for all connections

  # @req REQ-ONBOARD-007
  Scenario: HTTPS_PROXY environment variable is used when no proxy is configured in file
    Given "HTTPS_PROXY=https://proxy.corp.example:8080" is set in the environment
    And "~/.aegis/config.yaml" does not contain a proxy_url field
    When the user executes "aegis chat"
    Then all outbound connections should route through "https://proxy.corp.example:8080"

  # @req REQ-ONBOARD-007
  Scenario: AEGIS_CA_BUNDLE environment variable overrides config file CA bundle
    Given "AEGIS_CA_BUNDLE=/tmp/override-ca.pem" is set in the environment
    And "~/.aegis/config.yaml" specifies a different ca_bundle_path
    When the user executes "aegis chat"
    Then the CA bundle loaded should be "/tmp/override-ca.pem"

  # ---------------------------------------------------------------------------
  # REQ-ONBOARD-008: Environment variable overrides
  # ---------------------------------------------------------------------------

  # @req REQ-ONBOARD-008
  Scenario: AEGIS_ENDPOINT overrides the endpoint in config.yaml
    Given "~/.aegis/config.yaml" specifies endpoint "https://prod.endpoint.example"
    And the environment variable "AEGIS_ENDPOINT=https://staging.endpoint.example" is set
    When the user executes "aegis chat"
    Then the active endpoint should be "https://staging.endpoint.example"
    And the override should be logged at startup with the value redacted

  # @req REQ-ONBOARD-008
  Scenario: AEGIS_MODE overrides the mode in config.yaml
    Given "~/.aegis/config.yaml" specifies mode "self-service-byoc"
    And the environment variable "AEGIS_MODE=local" is set
    When the user executes "aegis chat"
    Then the active mode should be "local"

  # @req REQ-ONBOARD-008
  Scenario: Environment variable overrides are never written back to config.yaml
    Given "AEGIS_ENDPOINT=https://override.endpoint.example" is set in the environment
    And "~/.aegis/config.yaml" specifies a different endpoint
    When the user executes "aegis chat"
    Then "~/.aegis/config.yaml" should still contain the original endpoint value after the session

  # ---------------------------------------------------------------------------
  # REQ-ONBOARD-009: Config validation on startup
  # ---------------------------------------------------------------------------

  # @req REQ-ONBOARD-009
  Scenario: Missing required config field causes exit code 78
    Given "~/.aegis/config.yaml" exists but the "mode" field is absent
    When the user executes "aegis chat"
    Then aegis should exit with code 78
    And display "Required config field 'mode' is missing"
    And display a remediation hint to run "aegis init"

  # @req REQ-ONBOARD-009
  Scenario: Invalid endpoint URL causes exit code 78
    Given "~/.aegis/config.yaml" contains "endpoint: not-a-valid-url"
    When the user executes "aegis chat"
    Then aegis should exit with code 78
    And display "Config field 'endpoint' is not a valid HTTPS URL"

  # @req REQ-ONBOARD-009
  Scenario: Unknown mode value causes exit code 78
    Given "~/.aegis/config.yaml" contains "mode: banana"
    When the user executes "aegis chat"
    Then aegis should exit with code 78
    And display "Config field 'mode' must be one of: self-service-byoc, enterprise-byoc, managed-saas, local"

  # @req REQ-ONBOARD-009
  Scenario: Specified CA bundle path does not exist causes exit code 78
    Given "~/.aegis/config.yaml" contains "ca_bundle_path: /nonexistent/ca.pem"
    When the user executes "aegis chat"
    Then aegis should exit with code 78
    And display "ca_bundle_path '/nonexistent/ca.pem' does not exist"

  # ---------------------------------------------------------------------------
  # REQ-ONBOARD-010: Config schema migration
  # ---------------------------------------------------------------------------

  # @req REQ-ONBOARD-010
  Scenario: Config written by an older aegis version is auto-migrated on startup
    Given "~/.aegis/config.yaml" exists with "schema_version: 1"
    And the current aegis binary expects "schema_version: 2"
    When the user executes any "aegis" command
    Then aegis should back up the original config to "~/.aegis/config.yaml.v1.bak"
    And apply the v1-to-v2 migration
    And update "schema_version" to "2" in the live config file
    And log the migration event to the audit ledger

  # @req REQ-ONBOARD-010
  Scenario: Config migration failure leaves original config untouched
    Given "~/.aegis/config.yaml" exists with a corrupt schema_version field
    When aegis attempts migration on startup
    Then aegis should exit with code 78
    And display a descriptive migration error
    And the original config file should remain unmodified

  # ---------------------------------------------------------------------------
  # REQ-ONBOARD-011: First-run interactive tutorial
  # ---------------------------------------------------------------------------

  # @req REQ-ONBOARD-011
  Scenario: First-run tutorial is presented when no ~/.aegis/ directory exists
    Given no "~/.aegis/" directory exists on the system
    When the user executes "aegis init"
    Then the TUI wizard should display a welcome screen
    And walk the user through mode selection with descriptions
    And walk the user through a credential check step
    And walk the user through a connectivity test step
    And conclude with an invitation to run "aegis chat"
    And record "tutorial_completed: true" in "~/.aegis/config.yaml"

  # @req REQ-ONBOARD-011
  Scenario: First-run tutorial is suppressed in non-interactive mode
    Given no "~/.aegis/" directory exists on the system
    When the user executes "aegis init --non-interactive --mode local"
    Then the TUI wizard should not be displayed
    And the config should be written using provided flags and defaults

  # @req REQ-ONBOARD-011
  Scenario: Subsequent aegis init does not show tutorial again
    Given "~/.aegis/config.yaml" contains "tutorial_completed: true"
    When the user executes "aegis init"
    Then the TUI wizard welcome screen should not be displayed
    And the re-initialization update menu should be presented instead

  # ---------------------------------------------------------------------------
  # REQ-ONBOARD-012: Connectivity verification post-init
  # ---------------------------------------------------------------------------

  # @req REQ-ONBOARD-012
  Scenario: Post-init connectivity check reports success and latency
    Given a completed aegis init state machine
    When the Config Commit stage finishes
    Then aegis should send a minimal inference request to the configured endpoint
    And display "Connectivity verified (<latency>ms)"
    And record the connectivity check result in the audit ledger

  # @req REQ-ONBOARD-012
  Scenario: Post-init connectivity check failure sets health to degraded
    Given a completed aegis init state machine
    And the configured endpoint is unreachable
    When the Config Commit stage finishes
    Then aegis should display an actionable connectivity error
    And set "health: degraded" in "~/.aegis/config.yaml"
    And record the failure in the audit ledger
    And exit with a non-zero status code

  # @req REQ-ONBOARD-012
  Scenario: Connectivity check is skipped in air-gapped local mode
    Given the user executes "aegis init --local"
    When the Config Commit stage finishes
    Then no outbound network call should be made for connectivity verification

  # ---------------------------------------------------------------------------
  # REQ-ONBOARD-013: Enterprise BYOC mode
  # ---------------------------------------------------------------------------

  # @req REQ-ONBOARD-013
  Scenario: Enterprise BYOC mode records gateway URL and mTLS certificate
    Given the user executes "aegis init"
    And selects "Enterprise BYOC" mode
    And enters gateway URL "https://aegis-gw.corp.example"
    And selects "mTLS" as the auth method
    And provides certificate path "/etc/ssl/client.pem"
    Then "~/.aegis/config.yaml" should contain mode "enterprise-byoc"
    And contain "gateway_url: https://aegis-gw.corp.example"
    And contain a reference to the certificate path
    And no Pulumi provisioning should run

  # @req REQ-ONBOARD-013
  Scenario: Enterprise BYOC mode records gateway URL and service token reference
    Given the user executes "aegis init"
    And selects "Enterprise BYOC" mode
    And enters gateway URL "https://aegis-gw.corp.example"
    And selects "service_token" as the auth method
    And provides a service token
    Then the token should be stored in the OS keychain
    And "~/.aegis/config.yaml" should reference the keychain entry but not the token value

  # @req REQ-ONBOARD-013
  Scenario: Enterprise BYOC mode validates gateway TLS certificate chain
    Given the user executes "aegis init" in "Enterprise BYOC" mode
    And the gateway presents a self-signed TLS certificate not in the trust store
    When aegis attempts to validate the gateway URL
    Then aegis should display "TLS certificate verification failed for gateway_url"
    And prompt the user to provide a CA bundle or add the certificate to trust
    And not proceed past Credential Negotiation until TLS is valid

  # ---------------------------------------------------------------------------
  # REQ-ONBOARD-014: Managed SaaS mode with OAuth PKCE
  # ---------------------------------------------------------------------------

  # @req REQ-ONBOARD-014
  Scenario: Managed SaaS PKCE flow completes browser login and stores token reference
    Given the user executes "aegis init"
    And selects "Managed SaaS" mode
    When the OAuth PKCE flow initiates
    Then aegis should open a browser to "https://aegis.rtmx.ai/auth"
    And listen on a localhost ephemeral port for the OAuth callback
    And exchange the authorization code for access and refresh tokens
    And store the token in the OS keychain
    And record only the OIDC issuer URL in "~/.aegis/config.yaml"
    And not write any token value to "~/.aegis/config.yaml"

  # @req REQ-ONBOARD-014
  Scenario: Managed SaaS PKCE flow handles browser not available
    Given the user executes "aegis init" in "Managed SaaS" mode
    And no browser is available (headless server environment)
    When the OAuth PKCE flow initiates
    Then aegis should display the authorization URL for manual copy-paste
    And display instructions to paste the callback URL after browser authorization
    And complete the token exchange once the callback URL is pasted

  # @req REQ-ONBOARD-014
  Scenario: Managed SaaS PKCE flow times out if browser authorization not completed
    Given the user executes "aegis init" in "Managed SaaS" mode
    When the OAuth PKCE flow initiates
    And the user does not complete browser authorization within 5 minutes
    Then aegis should display "Authorization timed out"
    And no partial credentials should be stored
    And the config file should not be written

  # ---------------------------------------------------------------------------
  # REQ-ONBOARD-015: Multi-profile support
  # ---------------------------------------------------------------------------

  # @req REQ-ONBOARD-015
  Scenario: Multiple profiles can be created with distinct endpoints
    Given the user runs "aegis init --profile work" selecting "Self-Service BYOC"
    And later runs "aegis init --profile personal" selecting "managed-saas"
    When the user inspects "~/.aegis/config.yaml"
    Then it should contain a "profiles.work" section with mode "self-service-byoc"
    And a "profiles.personal" section with mode "managed-saas"

  # @req REQ-ONBOARD-015
  Scenario: --profile flag switches the active context for a command
    Given "~/.aegis/config.yaml" contains profiles "work" and "personal"
    When the user executes "aegis --profile work chat"
    Then the active endpoint and credentials should come from the "work" profile
    And the profile switch should be recorded in the audit ledger

  # @req REQ-ONBOARD-015
  Scenario: Default profile is used when --profile flag is omitted
    Given "~/.aegis/config.yaml" has a "default" profile and a "work" profile
    When the user executes "aegis chat" without a --profile flag
    Then the active profile should be "default"

  # @req REQ-ONBOARD-015
  Scenario: Specifying a non-existent profile exits with an error
    Given "~/.aegis/config.yaml" has only a "default" profile
    When the user executes "aegis --profile nonexistent chat"
    Then aegis should exit with code 78
    And display "Profile 'nonexistent' not found in config"

  # ---------------------------------------------------------------------------
  # REQ-ONBOARD-016: Config export and import for team sharing
  # ---------------------------------------------------------------------------

  # @req REQ-ONBOARD-016
  Scenario: Config export strips secrets and produces a shareable template
    Given a valid "~/.aegis/config.yaml" with a ca_bundle_path and proxy credentials
    When the user executes "aegis config export --output team-baseline.yaml"
    Then "team-baseline.yaml" should contain mode, cloud, region, endpoint, schema_version
    And it should not contain ca_bundle_path, proxy credentials, or keychain references
    And the file should be human-readable YAML

  # @req REQ-ONBOARD-016
  Scenario: Config import applies a team template and prompts for missing secrets
    Given "team-baseline.yaml" is a valid exported aegis config template
    When the user executes "aegis config import --file team-baseline.yaml"
    Then aegis should apply the template fields to "~/.aegis/config.yaml"
    And prompt interactively for any fields that require local secrets (e.g. CA bundle path)
    And set file permissions to 0600 on the resulting config

  # @req REQ-ONBOARD-016
  Scenario: Config import refuses to overwrite an existing config without confirmation
    Given "~/.aegis/config.yaml" already exists
    When the user executes "aegis config import --file team-baseline.yaml"
    Then aegis should warn that an existing config will be overwritten
    And require explicit confirmation before proceeding
    And back up the existing config before applying the import
