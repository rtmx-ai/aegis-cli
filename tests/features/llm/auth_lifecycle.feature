@wip
Feature: Auth Credential Lifecycle
  As a defense engineer using aegis with cloud LLM providers
  I need credentials to be managed transparently within the TUI
  So that I never leave aegis to authenticate, rotate, or refresh tokens

  Background:
    Given the aegis-llm crate is initialized with a valid configuration

  # ---------------------------------------------------------------------------
  # REQ-LLM-034: AuthManager shared credential store
  # ---------------------------------------------------------------------------

  # @req REQ-LLM-034
  Scenario: AuthManager caches resolved credentials per provider
    Given the AuthManager is initialized
    And the active provider is "vertex" with valid ADC credentials
    When the AuthManager resolves credentials for the first time
    Then it should cache the resolved ProviderAuth with an expires_at timestamp
    And subsequent resolve_or_refresh() calls should return the cached credential
    And no gcloud subprocess should be spawned for the second call

  # @req REQ-LLM-034
  Scenario: AuthManager emits Authenticated event on successful resolution
    Given the AuthManager is initialized with a status event channel
    And the active provider is "vertex"
    When the AuthManager resolves credentials successfully
    Then an AuthStatusEvent::Authenticated should be emitted on the status channel
    And the event should carry provider = Vertex and ttl_secs > 0

  # @req REQ-LLM-034
  Scenario: AuthManager reports expired credentials as invalid
    Given the AuthManager holds a cached credential for "vertex"
    And the credential's expires_at is in the past
    When is_valid(Vertex) is called
    Then it should return false
    And ttl(Vertex) should return Duration::ZERO

  # @req REQ-LLM-034
  Scenario: AuthManager serves multiple providers simultaneously
    Given the AuthManager holds cached credentials for "vertex" and "bedrock"
    When resolve_or_refresh() is called for each provider concurrently
    Then each provider should receive its own cached ProviderAuth
    And no cross-contamination of credentials should occur

  # @req REQ-LLM-034
  Scenario: AuthManager revoke clears cached credentials
    Given the AuthManager holds a cached credential for "vertex"
    When revoke(Vertex) is called
    Then is_valid(Vertex) should return false
    And the next resolve_or_refresh() should trigger a fresh auth resolution

  # ---------------------------------------------------------------------------
  # REQ-LLM-035: In-TUI device code auth flow
  # ---------------------------------------------------------------------------

  # @req REQ-LLM-035
  Scenario: GCP device code flow produces URL and user code
    Given the active provider is "vertex"
    And no valid ADC credentials are available
    When the AuthManager initiates device code auth for GCP
    Then a DeviceCodePending event should be emitted
    And the event should contain a verification_url starting with "https://"
    And the event should contain a non-empty user_code
    And the AuthManager should begin polling for token completion

  # @req REQ-LLM-035
  Scenario: AWS SSO device code flow produces URL and user code
    Given the active provider is "bedrock"
    And no valid AWS session credentials are available
    And AWS SSO is configured in the environment
    When the AuthManager initiates device code auth for AWS
    Then a DeviceCodePending event should be emitted
    And the event should contain the SSO verification URI
    And the event should contain a non-empty user_code

  # @req REQ-LLM-035
  Scenario: Azure device code flow produces URL and user code
    Given the active provider is "azure"
    And no valid Azure credentials are available
    When the AuthManager initiates device code auth for Azure
    Then a DeviceCodePending event should be emitted
    And the event should contain the Azure device login URL
    And the event should contain a non-empty user_code

  # @req REQ-LLM-035
  Scenario: Device code flow completes after user approves in browser
    Given the AuthManager is polling for a GCP device code token
    And the user approves the request in their browser
    When the next poll cycle executes
    Then the AuthManager should receive an access_token and refresh_token
    And a DeviceCodeComplete event should be emitted
    And the credential should be cached with the correct expires_at

  # @req REQ-LLM-035
  Scenario: Device code flow times out after 5 minutes
    Given the AuthManager is polling for a GCP device code token
    And the user does not approve within 5 minutes
    When the timeout elapses
    Then a RefreshFailed event should be emitted with reason containing "timeout"
    And the AuthManager should stop polling

  # @req REQ-LLM-035
  Scenario: Device code URL is rendered as OSC 8 clickable hyperlink
    Given a DeviceCodePending event is received by the TUI
    When the auth message is rendered in the chat log
    Then the verification URL should be wrapped in OSC 8 escape sequences
    And terminals supporting OSC 8 should render it as a clickable link

  # @req REQ-LLM-035
  Scenario: Local provider skips device code flow entirely
    Given the active provider is "local"
    When the AuthManager resolves credentials
    Then no device code flow should be initiated
    And no DeviceCodePending event should be emitted
    And ProviderAuth::NoAuth should be returned immediately

  # ---------------------------------------------------------------------------
  # REQ-LLM-036: Token TTL monitoring
  # ---------------------------------------------------------------------------

  # @req REQ-LLM-036
  Scenario: Status bar shows token TTL in green when more than 10 minutes remain
    Given the AuthManager reports TTL of 2820 seconds for "vertex"
    When the TUI renders the status bar
    Then it should display "GCP 47m" in green
    And the TTL should decrement locally on each tick event

  # @req REQ-LLM-036
  Scenario: Status bar shows token TTL in yellow when 2-10 minutes remain
    Given the AuthManager reports TTL of 480 seconds for "bedrock"
    When the TUI renders the status bar
    Then it should display "AWS 8m" in yellow

  # @req REQ-LLM-036
  Scenario: Status bar shows token TTL in red when less than 2 minutes remain
    Given the AuthManager reports TTL of 90 seconds for "azure"
    When the TUI renders the status bar
    Then it should display "Azure 1m" in red

  # @req REQ-LLM-036
  Scenario: Token TTL is not shown for local provider
    Given the active provider is "local"
    When the TUI renders the status bar
    Then no auth TTL indicator should be visible
    And no provider name should appear in the auth section

  # @req REQ-LLM-036
  Scenario: Expired token triggers system message in TUI
    Given the TTL for "vertex" reaches 0
    When the Expired event is received by the TUI
    Then a system message "Token expired. Refreshing..." should appear in the chat log
    And the AuthManager should attempt auto-refresh

  # ---------------------------------------------------------------------------
  # REQ-LLM-037: Background token auto-refresh
  # ---------------------------------------------------------------------------

  # @req REQ-LLM-037
  Scenario: Auto-refresh triggers when TTL falls below 120 seconds
    Given the AuthManager holds a GCP credential with TTL of 110 seconds
    And a valid refresh_token is stored in the keychain
    When the background refresh task runs its 30-second check cycle
    Then it should POST to the GCP token endpoint with grant_type=refresh_token
    And a new access_token should be cached with a fresh expires_at
    And an Authenticated event should be emitted with the new TTL

  # @req REQ-LLM-037
  Scenario: Auto-refresh is skipped when no refresh token is available
    Given the AuthManager holds a credential with TTL of 90 seconds
    And no refresh_token is stored for the provider
    When the background refresh task runs
    Then it should emit an ExpiryWarning event instead of attempting refresh
    And the credential should remain unchanged

  # @req REQ-LLM-037
  Scenario: Failed refresh emits RefreshFailed event
    Given the AuthManager holds a GCP credential with TTL of 60 seconds
    And the refresh_token has been revoked server-side
    When the background refresh task attempts to refresh
    Then the token endpoint should return an error
    And a RefreshFailed event should be emitted with the error reason
    And the user should see a system message with re-auth instructions

  # @req REQ-LLM-037
  Scenario: Refresh tokens are never logged or written to audit ledger
    Given a successful token refresh occurs
    When the refresh is recorded in the audit ledger
    Then the ledger entry should contain only the event type and provider name
    And no token values (access_token, refresh_token) should appear in any log file

  # @req REQ-LLM-037
  Scenario: Auto-refresh works across provider switch via /connect
    Given the user was connected to "vertex" with an active refresh cycle
    And the user runs "/connect bedrock --region=us-gov-west-1"
    When the provider switches to "bedrock"
    Then the refresh task should stop monitoring the vertex credential
    And begin monitoring the bedrock credential for expiry
    And the vertex credential should remain cached but dormant
