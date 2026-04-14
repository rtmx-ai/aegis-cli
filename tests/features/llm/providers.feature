@wip
Feature: LLM Provider Abstraction
  As a defense engineer using aegis in cloud and air-gapped environments
  I need a robust, provider-agnostic LLM interface
  So that the agent works reliably across Vertex AI, AWS Bedrock, Azure OpenAI, and local models

  Background:
    Given the aegis-llm crate is initialized with a valid configuration

  # ---------------------------------------------------------------------------
  # REQ-LLM-001: Multi-provider LLM abstraction with Vertex AI
  # ---------------------------------------------------------------------------

  # @req REQ-LLM-001
  Scenario: Vertex AI provider streams a response via the LlmProvider trait
    Given the active provider is "vertex"
    And the GCP Application Default Credentials are valid
    And the Vertex AI endpoint is configured for "us-central1"
    When the agent sends a conversation with one user message
    Then the provider should return a TokenStream
    And the stream should emit at least one Token event
    And the stream should terminate with a Done event carrying input_tokens and output_tokens

  # @req REQ-LLM-001
  Scenario: Provider factory selects Vertex AI from config
    Given the config contains provider = "vertex" and region = "us-central1"
    When the provider factory instantiates a provider
    Then the returned provider should be the Vertex AI implementation
    And the factory should not attempt network I/O during construction

  # @req REQ-LLM-001
  Scenario: Provider factory returns error for unknown provider name
    Given the config contains provider = "unknown_provider"
    When the provider factory instantiates a provider
    Then the factory should return a ConfigError containing "unknown provider: unknown_provider"

  # ---------------------------------------------------------------------------
  # REQ-LLM-002: AWS Bedrock provider
  # ---------------------------------------------------------------------------

  # @req REQ-LLM-002
  Scenario: Bedrock provider streams Claude via GovCloud endpoint
    Given the active provider is "bedrock"
    And valid AWS STS credentials are present for the GovCloud region "us-gov-west-1"
    And the model is set to "anthropic.claude-opus-4-6-v1"
    When the agent sends a conversation with one user message
    Then the provider should return a TokenStream
    And the stream should emit Token and Done events

  # @req REQ-LLM-002
  Scenario: Bedrock provider falls back to standard credential chain
    Given the active provider is "bedrock"
    And no explicit AWS_ACCESS_KEY_ID is set in the environment
    And an IAM instance profile is available on the host
    When the Bedrock provider is instantiated
    Then it should acquire credentials via the AWS SDK default credential chain
    And no credentials should appear in aegis logs or config

  # @req REQ-LLM-002
  Scenario: Bedrock provider fails with descriptive error when region is not GovCloud
    Given the active provider is "bedrock"
    And the region is set to "us-east-1" (commercial, not GovCloud)
    When the provider factory validates the config
    Then it should return a ConfigError containing "GovCloud region required for Bedrock"

  # ---------------------------------------------------------------------------
  # REQ-LLM-003: Azure OpenAI provider
  # ---------------------------------------------------------------------------

  # @req REQ-LLM-003
  Scenario: Azure OpenAI provider streams GPT-5.4 via Azure Government endpoint
    Given the active provider is "azure"
    And the Azure Government endpoint is "https://my-resource.openai.azure.us"
    And an Entra ID token is available via DefaultAzureCredential
    And the deployment name is set to "gpt-5-4"
    When the agent sends a conversation with one user message
    Then the provider should open a streaming SSE connection to the Azure endpoint
    And the stream should emit Token events followed by a Done event

  # @req REQ-LLM-003
  Scenario: Azure OpenAI provider authenticates with an API key when Entra ID is unavailable
    Given the active provider is "azure"
    And no Entra ID credentials are available
    And AZURE_OPENAI_API_KEY is set in the environment
    When the Azure provider is instantiated
    Then it should authenticate using the API key
    And the API key should not be written to any log or config file

  # @req REQ-LLM-003
  Scenario: Azure provider rejects non-government endpoint for IL4+ workloads
    Given the active provider is "azure"
    And the endpoint is "https://my-resource.openai.azure.com" (commercial)
    And the config specifies classification_level = "IL4"
    When the provider factory validates the config
    Then it should return a ConfigError containing "Azure Government endpoint required for IL4+"

  # ---------------------------------------------------------------------------
  # REQ-LLM-004: Local model provider
  # ---------------------------------------------------------------------------

  # @req REQ-LLM-004
  Scenario: Local provider reaches Ollama on the default endpoint
    Given the active provider is "local"
    And LOCAL_ENDPOINT is set to "http://localhost:11434"
    And an Ollama instance is running with model "llama3:8b"
    When the agent sends a conversation with one user message
    Then the provider should POST to the OpenAI-compatible /v1/chat/completions endpoint
    And no packets should leave the loopback interface
    And the stream should emit Token and Done events

  # @req REQ-LLM-004
  Scenario: Local provider fails fast when endpoint is unreachable
    Given the active provider is "local"
    And LOCAL_ENDPOINT points to a port with no listener
    When the agent sends a conversation
    Then the provider should return a ProviderError within the configured connect_timeout
    And the error message should include the endpoint URL

  # @req REQ-LLM-004
  Scenario: Local provider works with zero network egress
    Given the active provider is "local"
    And all non-loopback network routes are blocked
    When the agent sends a conversation to "http://127.0.0.1:11434"
    Then the request should succeed
    And no packets should leave the loopback interface

  # ---------------------------------------------------------------------------
  # REQ-LLM-005: Provider health checks
  # ---------------------------------------------------------------------------

  # @req REQ-LLM-005
  Scenario: Health check passes when provider endpoint is reachable
    Given any configured provider
    When "aegis health" or the provider factory calls health_check()
    Then the health check should return Healthy within 5 seconds
    And the result should include the provider name and model identifier

  # @req REQ-LLM-005
  Scenario: Health check reports degraded when endpoint returns 5xx
    Given the provider endpoint is returning HTTP 503
    When health_check() is invoked
    Then the health check should return Degraded
    And the result should include the HTTP status code in the error detail

  # @req REQ-LLM-005
  Scenario: Health check result is surfaced in the TUI status line
    Given the agent is running with a configured LLM provider
    When the provider health check completes
    Then the TUI status line should display "Provider: healthy" or "Provider: degraded"

  # ---------------------------------------------------------------------------
  # REQ-LLM-006: Model version pinning
  # ---------------------------------------------------------------------------

  # @req REQ-LLM-006
  Scenario: Provider uses the exact model version from config
    Given the config specifies model = "claude-opus-4-6@20250514"
    When the provider constructs the API request
    Then the request body should contain the literal model identifier "claude-opus-4-6@20250514"
    And the provider should not resolve aliases or latest tags

  # @req REQ-LLM-006
  Scenario: Provider rejects a missing model version in non-local mode
    Given the config omits the model field
    And the provider is not "local"
    When the provider factory instantiates the provider
    Then instantiation should return a ConfigError containing "model must be specified"

  # ---------------------------------------------------------------------------
  # REQ-LLM-007: Token counting per provider
  # ---------------------------------------------------------------------------

  # @req REQ-LLM-007
  Scenario: Done event carries per-request token counts
    Given any configured provider using the MockLlmProvider
    And the mock is configured to emit Done { input_tokens: 512, output_tokens: 128 }
    When the agent consumes the stream to completion
    Then the agent loop should record input_tokens = 512 and output_tokens = 128
    And those counts should be appended to the audit ledger entry for the request

  # @req REQ-LLM-007
  Scenario: Token counts accumulate across iterations in a session
    Given a session with three REA loop iterations
    And each iteration emits Done { input_tokens: 100, output_tokens: 50 }
    When the session ends
    Then the session total should be input_tokens = 300 and output_tokens = 150

  # ---------------------------------------------------------------------------
  # REQ-LLM-008: Cost tracking per session
  # ---------------------------------------------------------------------------

  # @req REQ-LLM-008
  Scenario: Session cost is computed from token counts and provider rate table
    Given the active provider is "vertex"
    And the model is "gemini-2.5-pro"
    And the session consumed 10000 input tokens and 2000 output tokens
    When the session ends
    Then the audit ledger entry should include estimated_cost_usd
    And the value should match the published Vertex AI pricing for gemini-2.5-pro

  # @req REQ-LLM-008
  Scenario: Cost is recorded as zero for local provider
    Given the active provider is "local"
    When the session ends
    Then the audit ledger entry should include estimated_cost_usd = 0.0

  # @req REQ-LLM-008
  Scenario: Accumulated session cost is displayed in the TUI status line
    Given a session in progress with a cloud provider
    When at least one LLM response has been received
    Then the TUI status line should show an estimated cost figure prefixed with "$"

  # ---------------------------------------------------------------------------
  # REQ-LLM-009: Streaming error recovery
  # ---------------------------------------------------------------------------

  # @req REQ-LLM-009
  Scenario: Provider recovers from a mid-stream connection reset
    Given the provider is streaming a long response
    And the underlying TCP connection resets after 20 tokens
    When the StreamEvent::Error is emitted on the channel
    Then the provider should attempt to resume or restart the stream up to max_stream_retries
    And partial tokens already delivered to the TUI should remain visible
    And a notice "Reconnecting to provider..." should appear in the TUI

  # @req REQ-LLM-009
  Scenario: Non-recoverable stream error surfaces as ProviderError after exhausting retries
    Given the provider is configured with max_stream_retries = 2
    And every stream attempt emits an Error event
    When retries are exhausted
    Then the provider should return a ProviderError to the agent loop
    And the error should be displayed to the user without crashing the TUI

  # ---------------------------------------------------------------------------
  # REQ-LLM-010: Request timeout configuration
  # ---------------------------------------------------------------------------

  # @req REQ-LLM-010
  Scenario: Request times out when the provider does not respond within connect_timeout
    Given the provider is configured with connect_timeout = 10s
    And the provider endpoint accepts the TCP connection but sends no data
    When the provider attempts to stream
    Then the provider should return a ProviderError after 10 seconds
    And the error message should include "connect timeout"

  # @req REQ-LLM-010
  Scenario: Read timeout fires when streaming stalls mid-response
    Given the provider is configured with read_timeout = 30s
    And the provider streams 10 tokens then stalls indefinitely
    When no token is received for 30 seconds
    Then the provider should cancel the stream and return a ProviderError
    And the error message should include "read timeout"

  # @req REQ-LLM-010
  Scenario: Timeout values are configurable in ~/.aegis/config.yaml
    Given the user sets connect_timeout = 5 and read_timeout = 60 in config
    When the provider is instantiated
    Then the provider should use exactly those timeout values for every request

  # ---------------------------------------------------------------------------
  # REQ-LLM-011: Retry with exponential backoff
  # ---------------------------------------------------------------------------

  # @req REQ-LLM-011
  Scenario: Provider retries on HTTP 429 with exponential backoff
    Given the provider endpoint returns HTTP 429 for the first two requests
    And retry policy is max_retries = 3 with base_delay = 500ms
    When the agent sends a request
    Then the provider should retry after approximately 500ms, then 1000ms
    And on the third attempt (which succeeds) the agent should receive a valid stream
    And the retry count should be recorded in the audit ledger

  # @req REQ-LLM-011
  Scenario: Provider does not retry on HTTP 400 (bad request)
    Given the provider endpoint returns HTTP 400
    When the agent sends a request
    Then the provider should return a ProviderError immediately without retrying
    And the error should include the HTTP 400 response body

  # @req REQ-LLM-011
  Scenario: Retry respects Retry-After header from provider
    Given the provider endpoint returns HTTP 429 with "Retry-After: 2"
    When the provider receives the 429 response
    Then the retry delay should be at least 2 seconds regardless of the computed backoff

  # ---------------------------------------------------------------------------
  # REQ-LLM-012: Provider failover
  # ---------------------------------------------------------------------------

  # @req REQ-LLM-012
  Scenario: Agent fails over to fallback provider when primary is unavailable
    Given the config defines primary = "vertex" and fallback = "bedrock"
    And the Vertex AI endpoint returns HTTP 503 on all retries
    When the agent sends a request
    Then the provider layer should transparently switch to the Bedrock provider
    And an audit ledger entry should record the failover event with provider names and timestamp
    And the TUI should display "Provider failover: vertex -> bedrock"

  # @req REQ-LLM-012
  Scenario: Failover is skipped in air-gapped local mode
    Given the config defines mode = "local" and provider = "local"
    When the local provider is unavailable
    Then no failover should be attempted
    And the error should indicate the local endpoint is unreachable

  # @req REQ-LLM-012
  Scenario: Failover does not occur for 4xx client errors
    Given the config defines primary = "vertex" and fallback = "bedrock"
    And the Vertex AI endpoint returns HTTP 401
    When the agent sends a request
    Then no failover should be attempted
    And the error should indicate an authentication failure for the primary provider

  # ---------------------------------------------------------------------------
  # REQ-LLM-013: Response validation and sanitization
  # ---------------------------------------------------------------------------

  # @req REQ-LLM-013
  Scenario: Provider rejects a response that exceeds maximum token limit
    Given the provider is configured with max_response_tokens = 8192
    And the provider streams a Done event with output_tokens = 9000
    When the stream completes
    Then the provider should return a ProviderError indicating the response exceeded limits
    And the oversized response should not be injected into conversation history

  # @req REQ-LLM-013
  Scenario: Provider strips null bytes and control characters from token stream
    Given a provider stream that emits tokens containing null bytes (U+0000)
    When those tokens are relayed to the TUI
    Then null bytes and non-printable control characters (except LF and TAB) should be removed
    And the sanitized token should be passed downstream without error

  # @req REQ-LLM-013
  Scenario: Malformed tool call JSON from the LLM is rejected gracefully
    Given the LLM emits a ToolUse event with syntactically invalid JSON parameters
    When the agent loop attempts to deserialize the tool call
    Then the agent should emit a Tool role message with an error result
    And no tool execution should occur
    And the agent should continue the REA loop asking the LLM to retry

  # ---------------------------------------------------------------------------
  # REQ-LLM-014: Prompt caching support
  # ---------------------------------------------------------------------------

  # @req REQ-LLM-014
  Scenario: Vertex AI and Bedrock providers attach cache_control to eligible messages
    Given the conversation history contains a system prompt longer than 1024 tokens
    And the active provider supports prompt caching (Vertex AI or Bedrock Claude)
    When the provider constructs the API request
    Then the system message should carry a cache_control marker in the request body
    And the Done event should include cache_read_input_tokens when the cache was hit

  # @req REQ-LLM-014
  Scenario: Cache hit tokens are counted separately in the audit ledger
    Given prompt caching is enabled
    And the Done event carries cache_read_input_tokens = 2000 and input_tokens = 100
    When the session audit entry is written
    Then it should record cache_read_input_tokens = 2000 separately from billed input_tokens = 100

  # @req REQ-LLM-014
  Scenario: Prompt caching is disabled for the local provider
    Given the active provider is "local"
    When the provider constructs the API request
    Then no cache_control markers should be attached to any messages

  # ---------------------------------------------------------------------------
  # REQ-LLM-015: Provider-specific authentication flows
  # ---------------------------------------------------------------------------

  # @req REQ-LLM-015
  Scenario: Vertex AI authenticates using Application Default Credentials (ADC)
    Given the active provider is "vertex"
    And GOOGLE_APPLICATION_CREDENTIALS points to a valid service account key
    When the provider makes a request
    Then it should obtain a Bearer token via the google-auth library
    And the token should be included in the Authorization header

  # @req REQ-LLM-015
  Scenario: Vertex AI ADC token is refreshed before expiry
    Given the active provider is "vertex"
    And the current ADC token expires in 60 seconds
    When the provider prepares a request
    Then it should proactively refresh the token if expiry is within 120 seconds
    And the refreshed token should be used for the outgoing request

  # @req REQ-LLM-015
  Scenario: Bedrock authenticates using STS AssumeRole when role_arn is configured
    Given the active provider is "bedrock"
    And the config contains role_arn = "arn:aws-us-gov:iam::123456789012:role/AegisRole"
    When the provider is instantiated
    Then it should call STS AssumeRole with the configured ARN
    And the resulting session credentials should be used for Bedrock API calls

  # @req REQ-LLM-015
  Scenario: Azure OpenAI prefers Entra ID over API key when both are present
    Given the active provider is "azure"
    And both AZURE_OPENAI_API_KEY and a valid Entra ID token are available
    When the Azure provider is instantiated
    Then it should authenticate via Entra ID (DefaultAzureCredential)
    And it should not include the API key in request headers

  # @req REQ-LLM-015
  Scenario: Local provider requires no authentication
    Given the active provider is "local"
    When the provider is instantiated
    Then no authentication headers should be set
    And no credential files should be accessed

  # ---------------------------------------------------------------------------
  # REQ-LLM-016: Endpoint URL validation
  # ---------------------------------------------------------------------------

  # @req REQ-LLM-016
  Scenario: Provider factory rejects an endpoint URL with a non-HTTPS scheme in cloud mode
    Given the config sets endpoint = "http://my-provider.example.com" (HTTP, not HTTPS)
    And the provider is not "local"
    When the provider factory instantiates the provider
    Then it should return a ConfigError containing "endpoint must use HTTPS"
    And no network connection should be attempted

  # @req REQ-LLM-016
  Scenario: Local provider accepts HTTP endpoints on loopback addresses
    Given the config sets provider = "local" and endpoint = "http://127.0.0.1:11434"
    When the provider factory instantiates the local provider
    Then no ConfigError should be returned
    And the provider should use the HTTP endpoint as configured

  # @req REQ-LLM-016
  Scenario: Provider factory rejects a syntactically invalid endpoint URL
    Given the config sets endpoint = "not-a-url"
    When the provider factory instantiates any provider
    Then it should return a ConfigError containing "invalid endpoint URL"

  # @req REQ-LLM-016
  Scenario: Local provider rejects HTTP on non-loopback addresses
    Given the config sets provider = "local" and endpoint = "http://192.168.1.100:11434"
    When the provider factory instantiates the local provider
    Then it should return a ConfigError containing "HTTP allowed only for loopback addresses"

  # ---------------------------------------------------------------------------
  # REQ-LLM-017: Model capability detection
  # ---------------------------------------------------------------------------

  # @req REQ-LLM-017
  Scenario: Provider reports tool_use capability for Claude and GPT-5.4 models
    Given the active model is known to support function calling (Claude or GPT-5.4)
    When the agent queries provider.capabilities()
    Then capabilities should include ToolUse = true

  # @req REQ-LLM-017
  Scenario: Agent enables ToolShim when provider reports ToolUse = false
    Given the active provider is "local"
    And the local model reports ToolUse = false
    When the agent loop initializes
    Then the ToolShim should be enabled automatically
    And the agent should not send a tools array in the raw API request

  # @req REQ-LLM-017
  Scenario: Provider reports context window size for the configured model
    Given the active model is "claude-opus-4-6@20250514"
    When the agent queries provider.capabilities()
    Then capabilities should include context_window_tokens = 200000

  # ---------------------------------------------------------------------------
  # REQ-LLM-018: Context window size awareness
  # ---------------------------------------------------------------------------

  # @req REQ-LLM-018
  Scenario: Agent truncates oldest history messages when approaching context window limit
    Given the model has a context_window_tokens of 8192
    And the accumulated conversation history exceeds 7000 tokens
    When the agent prepares the next API request
    Then the oldest non-system messages should be evicted until the payload is under 7000 tokens
    And a notice "Context truncated: N messages removed" should appear in the TUI

  # @req REQ-LLM-018
  Scenario: System prompt is never evicted during context window truncation
    Given the model has a context_window_tokens of 8192
    And truncation is required
    When the agent evicts messages
    Then the system-role message should remain at position 0 in the history

  # @req REQ-LLM-018
  Scenario: Truncation triggers at 85% threshold
    Given the model has a context_window_tokens of 10000
    And the conversation history is at 8500 tokens (85%)
    When the agent prepares the next request
    Then truncation should trigger
    And the resulting payload should be below 8500 tokens

  # ---------------------------------------------------------------------------
  # REQ-LLM-019: Connection pooling
  # ---------------------------------------------------------------------------

  # @req REQ-LLM-019
  Scenario: Provider reuses HTTP connections across sequential requests
    Given the active provider is any cloud provider
    And two sequential LLM requests are made in the same session
    When the requests are traced at the TCP level
    Then both requests should use the same underlying TCP connection (Keep-Alive)
    And no new TLS handshake should be performed for the second request

  # @req REQ-LLM-019
  Scenario: Connection pool size is bounded by the configured max_connections value
    Given the config sets max_connections = 4 for the provider
    And five concurrent requests are initiated
    When the requests execute
    Then at most 4 simultaneous TCP connections should be open to the provider endpoint

  # ---------------------------------------------------------------------------
  # Record/Replay: CI determinism
  # ---------------------------------------------------------------------------

  # @req REQ-LLM-001
  Scenario: TestProvider replays a recorded Vertex AI interaction deterministically
    Given a wiremock recording exists at "tests/recordings/vertex_streaming_001.json"
    When the TestProvider replays the recording
    Then the emitted StreamEvents should be byte-for-byte identical to the original recording
    And no network connection should be made during replay

  # @req REQ-LLM-002
  Scenario: TestProvider replays a recorded Bedrock interaction deterministically
    Given a wiremock recording exists at "tests/recordings/bedrock_claude_001.json"
    When the TestProvider replays the recording
    Then the emitted StreamEvents should be byte-for-byte identical to the original recording
    And no network connection should be made during replay
