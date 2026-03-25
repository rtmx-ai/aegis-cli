Feature: Human-in-the-Loop Approval Gate
  As a defense engineer operating aegis on networks handling CUI
  I need every state-mutating operation to require explicit human approval
  So that no unauthorized file writes or command executions occur

  # ---------------------------------------------------------------------------
  # REQ-HITL-001: HITL gate blocks all state-mutating tool calls
  # ---------------------------------------------------------------------------

  # @req REQ-HITL-001
  Scenario: write_file blocked until user approves
    Given the agent decides to invoke "write_file" on "src/config.rs" with new content
    When the HITL gate activates
    Then an inline approval dialog should appear with options [Y] Approve [N] Deny [E] Edit [S] Skip
    And the event loop should block until the user responds
    And no bytes should be written to "src/config.rs" before approval

  # @req REQ-HITL-001
  Scenario: User denies a write operation
    Given the HITL gate is displaying an approval dialog for "write_file" on "src/config.rs"
    When the user presses "N" to deny
    Then the file "src/config.rs" should not be modified
    And the denial should be logged to the audit ledger with event type "HITL_DENIED"
    And the agent should continue its loop with the denial as feedback

  # @req REQ-HITL-001
  Scenario: User edits a proposed write before approving
    Given the HITL gate is displaying an approval dialog for "write_file"
    When the user presses "E" to edit
    Then an editor should open with the proposed content
    And after editing and saving, the modified content should be written
    And the audit ledger should record "HITL_APPROVED_WITH_EDITS"

  # @req REQ-HITL-001
  Scenario: User skips a tool call
    Given the HITL gate is displaying an approval dialog for "run_command"
    When the user presses "S" to skip
    Then the command should not execute
    And the agent should receive a "tool call skipped by user" result
    And the audit ledger should record "HITL_SKIPPED"

  # @req REQ-HITL-001
  Scenario: run_command blocked until user approves
    Given the agent decides to invoke "run_command" with "cargo build"
    When the HITL gate activates
    Then the approval dialog should display the exact command "cargo build"
    And the command should not execute until the user presses "Y"

  # @req REQ-HITL-001
  Scenario: HITL approval is logged to audit ledger
    Given the user approves a "write_file" operation
    When the approval is recorded
    Then the audit ledger should contain an entry with event type "HITL_APPROVED"
    And the entry should include the tool name, target path, and timestamp

  # ---------------------------------------------------------------------------
  # REQ-HITL-002: Configurable permission rules with graduated trust
  # ---------------------------------------------------------------------------

  # @req REQ-HITL-002
  Scenario: Session-persistent grant allows repeated writes without re-prompting
    Given the user approves "write_file" for path pattern "src/*.rs" with "allow for session"
    When the agent later invokes "write_file" on "src/lib.rs"
    Then the operation should proceed without a new approval dialog
    And the auto-approval should be logged as "HITL_AUTO_APPROVED (session grant)"

  # @req REQ-HITL-002
  Scenario: Mode cycling between Ask, AcceptEdits, and FullAuto
    Given the current permission mode is "Ask"
    When the user cycles the mode to "AcceptEdits"
    Then all file edit operations should auto-approve
    But command execution should still require approval

  # @req REQ-HITL-002
  Scenario: FullAuto mode is not available in production configuration
    Given the configuration indicates a production deployment
    When the user attempts to set permission mode to "FullAuto"
    Then aegis should reject the mode change
    And display "FullAuto mode is disabled in production deployments"

  # @req REQ-HITL-002
  Scenario: Deny rule blocks operations matching a path pattern
    Given a deny rule exists for path pattern "~/.ssh/*"
    When the agent invokes "read_file" on "~/.ssh/id_rsa"
    Then the operation should be denied immediately without prompting
    And the audit ledger should record "HITL_DENIED (policy)"

  # ---------------------------------------------------------------------------
  # REQ-HITL-003: HITL approval timeout with auto-deny
  # ---------------------------------------------------------------------------

  # @req REQ-HITL-003
  Scenario: Unattended dialog auto-denies after 60 seconds
    Given the HITL gate displays an approval dialog
    And the user does not respond
    When 60 seconds elapse
    Then the operation should be denied automatically
    And the audit ledger should record "HITL_TIMEOUT" distinct from "HITL_DENIED"

  # @req REQ-HITL-003
  Scenario: Custom timeout overrides the default
    Given config contains "hitl_timeout_seconds: 30"
    And the HITL gate displays an approval dialog
    When 30 seconds elapse without user response
    Then the operation should be denied automatically

  # @req REQ-HITL-003
  Scenario: Timer is visible in the approval dialog
    Given the HITL gate displays an approval dialog with a 60-second timeout
    When 10 seconds have elapsed
    Then the dialog should display a countdown showing approximately "50s remaining"

  # ---------------------------------------------------------------------------
  # REQ-HITL-004: Batch approval for homogeneous tool call sequences
  # ---------------------------------------------------------------------------

  # @req REQ-HITL-004
  Scenario: Batch approval for consecutive same-tool calls
    Given the agent has queued 5 consecutive "write_file" calls
    When the HITL gate activates
    Then the dialog should offer "[A] Approve All" in addition to individual options
    And pressing "A" should approve all 5 operations
    And the audit ledger should record a single "HITL_BATCH_APPROVED" event with count 5

  # @req REQ-HITL-004
  Scenario: Batch approval is not offered for mixed tool types
    Given the agent has queued "write_file" then "run_command"
    When the HITL gate activates for the first call
    Then the "[A] Approve All" option should not be offered
    And each tool call should be presented individually

  # @req REQ-HITL-004
  Scenario: Batch approval requires at least 2 consecutive same-tool calls
    Given the agent has queued only 1 "write_file" call
    When the HITL gate activates
    Then the "[A] Approve All" option should not be offered

  # ---------------------------------------------------------------------------
  # REQ-HITL-005: Rollback journal for approved write operations
  # ---------------------------------------------------------------------------

  # @req REQ-HITL-005
  Scenario: Snapshot is taken before an approved write operation
    Given a file "src/main.rs" exists with original content
    When the user approves a "write_file" operation on "src/main.rs"
    Then the original content should be saved to "~/.aegis/rollback/<session_id>/src/main.rs"
    And then the new content should be written

  # @req REQ-HITL-005
  Scenario: aegis undo restores the previous file version
    Given "src/main.rs" was modified by an approved write operation
    And the rollback journal contains the original content
    When the user executes "aegis undo"
    Then "src/main.rs" should be restored to its original content
    And the audit ledger should record a "RollbackExecuted" event

  # @req REQ-HITL-005
  Scenario: aegis undo fails gracefully when no rollback data exists
    Given no rollback journal entries exist for the current session
    When the user executes "aegis undo"
    Then aegis should display "No operations to undo in current session"
    And exit with a non-zero code

  # @req REQ-HITL-005
  Scenario: Rollback journal preserves file for new file creation
    Given "new_file.rs" does not exist
    When the user approves a "write_file" operation creating "new_file.rs"
    Then the rollback journal should record that "new_file.rs" did not previously exist
    And "aegis undo" should delete "new_file.rs"

  # ---------------------------------------------------------------------------
  # REQ-HITL-006: Approval history review command
  # ---------------------------------------------------------------------------

  # @req REQ-HITL-006
  Scenario: aegis history lists all approval decisions for current session
    Given the current session has 3 approvals and 2 denials
    When the user runs "aegis history"
    Then the output should list 5 entries with timestamps, tool names, and decisions
    And each entry should show either "APPROVED", "DENIED", or "SKIPPED"

  # @req REQ-HITL-006
  Scenario: aegis history --denied filters to denied operations only
    Given the current session has 3 approvals and 2 denials
    When the user runs "aegis history --denied"
    Then only the 2 denied entries should be displayed

  # @req REQ-HITL-006
  Scenario: aegis history --session filters by session ID
    Given there are 3 sessions in the audit ledger
    When the user runs "aegis history --session <session_id>"
    Then only entries from the specified session should be displayed

  # ---------------------------------------------------------------------------
  # REQ-HITL-007: Emergency kill switch halts agent via Ctrl+K
  # ---------------------------------------------------------------------------

  # @req REQ-HITL-007
  Scenario: Ctrl+K terminates the agent loop immediately
    Given the agent is executing with 3 queued tool calls
    When the user presses Ctrl+K
    Then the agent loop should terminate immediately
    And all 3 queued tool calls should be denied
    And the audit ledger should record "KillSwitchActivated" with aborted_count: 3

  # @req REQ-HITL-007
  Scenario: Ctrl+K during a running tool aborts the tool
    Given the agent is executing a long-running "run_command"
    When the user presses Ctrl+K
    Then the running command should be killed
    And no further tool calls should execute
    And the exit should be clean without partial writes

  # @req REQ-HITL-007
  Scenario: Kill switch is distinct from Ctrl+C cancellation
    Given the agent is running
    When the user presses Ctrl+K
    Then the behavior should differ from Ctrl+C in that all queued calls are denied
    And the audit event type should be "KillSwitchActivated" not "UserCancelled"

  # ---------------------------------------------------------------------------
  # REQ-HITL-008: Persistent session grants survive restarts within 24h
  # ---------------------------------------------------------------------------

  # @req REQ-HITL-008
  Scenario: Session grant persists across restart within 24 hours
    Given the user granted "write_file" for "src/*.rs" with persistence
    And the grant was created 12 hours ago
    When the user restarts aegis and the agent invokes "write_file" on "src/lib.rs"
    Then the operation should auto-approve using the persisted grant
    And the grant should be loaded from "~/.aegis/grants.json"

  # @req REQ-HITL-008
  Scenario: Expired grant is purged on load
    Given "~/.aegis/grants.json" contains a grant created 25 hours ago
    When aegis starts and loads grants
    Then the expired grant should be removed from "~/.aegis/grants.json"
    And the tool call should require fresh approval

  # @req REQ-HITL-008
  Scenario: Grants file has 0600 permissions
    Given "~/.aegis/grants.json" exists
    When I check the file permissions
    Then the permissions should be 0600
    And aegis should reject the file if permissions are more permissive

  # @req REQ-HITL-008
  Scenario: Corrupt grants file is ignored and regenerated
    Given "~/.aegis/grants.json" contains invalid JSON
    When aegis starts and attempts to load grants
    Then the corrupt file should be renamed to "~/.aegis/grants.json.corrupt"
    And a new empty grants file should be created
    And a warning should be displayed about the corrupted grants
