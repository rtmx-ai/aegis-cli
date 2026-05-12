## Tool inventory

aegis provides the following built-in tools for interacting with the local
development environment:

- **File read** -- read file contents with line-number output, supporting offset
  and limit for large files.
- **File write** -- create new files or overwrite existing files. Blocked by
  `.aegisignore` for sensitive paths.
- **File edit** -- apply targeted string replacements within existing files.
  Shows a diff preview in the HITL approval prompt.
- **Shell command execution** -- run arbitrary shell commands in the user's
  environment. All commands require HITL approval. Commands are logged with
  their exit code and truncated output in the audit ledger.
- **Grep search** -- regex-based content search across files, powered by
  ripgrep. Supports glob filtering, file type filtering, context lines, and
  multiple output modes.
- **Glob search** -- fast file pattern matching for locating files by name.
  Returns results sorted by modification time.
- **Diff generation** -- produce unified diffs between file versions for review.

Tools are dispatched through a `ToolExecutor` port. Each invocation is assigned
a `ToolRisk` classification (read-only, write, or destructive) that determines
whether HITL approval is required.

## Plugin protocol: aegis-infra/v1

aegis does not embed infrastructure-as-code engines. Instead, it hosts IaC
plugins as separate binaries spawned as child processes. Communication uses
newline-delimited JSON (NDJSON) on the plugin's stdout.

Plugin subcommands: `manifest`, `preview`, `up`, `status`, `destroy`.

Plugin event types emitted on stdout:
- `progress` -- lifecycle phase advancement and percentage updates.
- `diagnostic` -- warnings and informational messages from the plugin.
- `check` -- individual resource health check results.
- `result` -- final operation outcome with resource inventory.

Plugin lifecycle phases run in strict order:
1. **PREFLIGHT** -- validate credentials, permissions, and prerequisites.
2. **API_ENABLEMENT** -- enable required cloud APIs or services.
3. **PROVISION** -- create, update, or destroy infrastructure resources.
4. **VERIFY** -- run health checks against provisioned resources.

The reference plugin implementation is gcp-assured-workloads, which provisions
GCP Assured Workloads environments at IL4/IL5. Plugin authors use the
@aegis/infra-sdk package, implementing three interfaces: CspClient, IaCEngine,
and HealthChecker.

## Deployment mode authentication details

- **Managed** -- OAuth 2.0 Authorization Code with PKCE. The TUI opens a
  localhost callback listener, launches the system browser to the identity
  provider, and exchanges the authorization code for tokens. Tokens are stored
  in the OS keychain.
- **Self-managed** -- customer OIDC or SAML federation, or static API key.
  Configuration is set during `aegis init` and persisted in the aegis config
  file. Token refresh is automatic for OIDC; SAML sessions are re-established
  on expiry.
- **Air-gapped** -- X.509 client certificates (including CAC/PIV via PKCS#11)
  or static API key. No external network calls are made. The LLM endpoint
  is a local address (e.g., localhost:11434 for Ollama).

## Audit behavior

Every tool call generates an audit entry containing: timestamp, session ID,
tool name, risk classification, approval decision, and execution duration.
Every LLM interaction generates an entry containing: timestamp, session ID,
provider name, model identifier, token counts (prompt and completion), and
latency. CUI content is never written to the ledger. When the security filter
blocks a transmission, a `CuiBlocked` event is recorded with the blocking
reason and destination classification.

The ledger is append-only. Concurrent writes are serialized with file-level
locking. Size-based rotation creates new ledger segments when the current file
exceeds the configured threshold, preserving older segments for archival.

## Slash commands

The TUI supports the following slash commands for session control:

- `/model` -- switch the active LLM model or list available models.
- `/connect` -- establish or re-establish connection to an LLM endpoint.
- `/clear` -- clear the conversation history and start a fresh context.
- `/add` -- add files or directories to the conversation context.
- `/drop` -- remove files from the conversation context.
- `/context` -- display the current context window: loaded files, token usage,
  and remaining budget.
- `/help` -- display available commands and keyboard shortcuts.

## Requirements traceability

aegis integrates with RTMX for requirements traceability. Every test function
carries an `// rtmx:req REQ-XXX-NNN` marker linking it to a tracked
requirement. The `rtmx status` command reports coverage, and CI enforces that
no requirement regresses from COMPLETE to an earlier lifecycle state.
