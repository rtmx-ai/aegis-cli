Feature: Immutable Local Audit Ledger
  As a compliance officer reviewing an aegis deployment
  I need an immutable, structured log of all agent actions
  So that I can reconstruct the exact body of evidence for any session

  # ---------------------------------------------------------------------------
  # REQ-AUDIT-001: Immutable local audit ledger at ~/.aegis/logs/*.jsonl
  # ---------------------------------------------------------------------------

  # @req REQ-AUDIT-001
  Scenario: Session start and stop are logged
    Given a new aegis session
    When the session starts
    Then the audit ledger should contain a SESSION_START entry
    And the entry should include session_id, timestamp, and os_user
    When the session ends
    Then the audit ledger should contain a SESSION_END entry with the same session_id

  # @req REQ-AUDIT-001
  Scenario: Tool calls and approvals are logged with metadata only
    Given an aegis session with a mock LLM
    When the agent proposes writing to "src/fix.rs"
    And the user approves the write
    Then the audit ledger should contain entries for:
      | event_type        | details                    |
      | TOOL_PROPOSED     | write_file: src/fix.rs     |
      | TOOL_APPROVED     | decision: approved         |
      | TOOL_EXECUTED     | result: success            |

  # @req REQ-AUDIT-001
  Scenario: Ledger never contains file contents, prompts, or AI responses
    Given an aegis session that reads "src/classified.rs"
    And "src/classified.rs" contains sensitive source code
    When the session completes
    Then the audit ledger should contain a CONTEXT_READ entry for "src/classified.rs"
    But the audit ledger should not contain any file contents
    And the audit ledger should not contain any LLM prompts or responses
    And the audit ledger should not contain any stdout output

  # @req REQ-AUDIT-001
  Scenario: Ledger is append-only JSONL format
    Given an aegis session that performs multiple actions
    When I read the ledger file "~/.aegis/logs/current.jsonl"
    Then each line should be valid JSON
    And the file should be parseable by standard JSONL tools
    And no previous entries should be modified or deleted

  # @req REQ-AUDIT-001
  Scenario: Token counts are logged per request
    Given an aegis session with an LLM request
    When the LLM responds with input_tokens: 1000 and output_tokens: 500
    Then the audit ledger should contain a TOKENS_CONSUMED entry
    And the entry should include input_tokens: 1000 and output_tokens: 500

  # ---------------------------------------------------------------------------
  # REQ-AUDIT-002: Cloud audit logs via provider-native logging
  # ---------------------------------------------------------------------------

  # @req REQ-AUDIT-002
  @wip
  Scenario: GCP Cloud Audit Logs record Vertex AI API access
    Given aegis is configured with a GCP Assured Workloads boundary
    When the agent sends a request to Vertex AI
    Then GCP Cloud Audit Logs should contain a log entry for the API call
    And the entry should include the IAM identity and source IP
    And the log should be stored in the CMEK-encrypted audit bucket

  # @req REQ-AUDIT-002
  @wip
  Scenario: AWS CloudTrail records Bedrock API access
    Given aegis is configured with an AWS GovCloud boundary
    When the agent sends a request to Bedrock
    Then AWS CloudTrail should contain a log entry for the API call
    And the entry should include the IAM role ARN and source IP

  # @req REQ-AUDIT-002
  @wip
  Scenario: Cloud audit logs prove authorized access from authorized IP
    Given a complete cloud audit trail for a session
    When a compliance auditor reviews the logs
    Then every API call should show an authorized IAM identity
    And every API call should originate from an IP within the configured VPC

  # ---------------------------------------------------------------------------
  # REQ-AUDIT-003: Audit entries link to RTMX requirement IDs
  # ---------------------------------------------------------------------------

  # @req REQ-AUDIT-003
  @wip
  Scenario: Ledger entries include req_id when working on a requirement
    Given the user sends "aegis implement REQ-HITL-001"
    When the agent executes tool calls during implementation
    Then each audit ledger entry for that session should contain req_id: "REQ-HITL-001"

  # @req REQ-AUDIT-003
  @wip
  Scenario: Ledger entries omit req_id when no requirement is active
    Given the user sends a general prompt without referencing a requirement
    When the agent executes tool calls
    Then the audit ledger entries should not contain a req_id field

  # ---------------------------------------------------------------------------
  # REQ-AUDIT-004: Log rotation daily and at 10 MB size threshold
  # ---------------------------------------------------------------------------

  # @req REQ-AUDIT-004
  @wip
  Scenario: Ledger rotates at 10 MB size threshold
    Given the active ledger file has reached 10 MB
    When a new entry is appended
    Then the current file should be closed with a timestamp suffix
    And a new active ledger file should be created
    And the first entry in the new file should be SESSION_CONTINUE

  # @req REQ-AUDIT-004
  @wip
  Scenario: Ledger rotates daily at midnight
    Given the active ledger file was created yesterday
    When the first entry of a new day is written
    Then the previous day's file should be closed with a date suffix
    And a new active ledger file should be created

  # @req REQ-AUDIT-004
  @wip
  Scenario: No data loss occurs during rotation
    Given a rotation is triggered while entries are being written
    When the rotation completes
    Then every entry written before rotation should be in the closed file
    And every entry written after should be in the new file
    And no entries should be duplicated or lost

  # ---------------------------------------------------------------------------
  # REQ-AUDIT-005: SHA-256 chain integrity per ledger entry
  # ---------------------------------------------------------------------------

  # @req REQ-AUDIT-005
  @wip
  Scenario: Each ledger entry contains prev_hash linking to predecessor
    Given a ledger with 10 entries
    When I inspect each entry
    Then entry N should contain prev_hash equal to the SHA-256 of entry N-1
    And entry 0 (genesis) should have prev_hash: null

  # @req REQ-AUDIT-005
  @wip
  Scenario: aegis audit verify detects tampered entry
    Given a ledger with 10 entries
    And entry 5 has been manually modified
    When the user runs "aegis audit verify"
    Then the command should report "Chain integrity broken at entry 6: prev_hash mismatch"
    And exit with a non-zero code

  # @req REQ-AUDIT-005
  @wip
  Scenario: aegis audit verify passes on an untampered ledger
    Given a ledger with 100 entries and no modifications
    When the user runs "aegis audit verify"
    Then the command should report "Ledger integrity verified: 100 entries, chain intact"
    And exit with code 0

  # ---------------------------------------------------------------------------
  # REQ-AUDIT-006: User identity binding per session
  # ---------------------------------------------------------------------------

  # @req REQ-AUDIT-006
  @wip
  Scenario: SESSION_START entry includes OS user and hostname
    Given a new aegis session started by user "jdoe" on host "dev-workstation"
    When the SESSION_START entry is written
    Then it should contain os_user: "jdoe"
    And hostname: "dev-workstation"
    And uid matching the OS user ID

  # @req REQ-AUDIT-006
  @wip
  Scenario: Identity fields are consistent across all entries in a session
    Given a session with 20 entries
    When I inspect the session_id in each entry
    Then all entries should share the same session_id
    And the identity should be attributable via the SESSION_START entry

  # ---------------------------------------------------------------------------
  # REQ-AUDIT-007: Concurrent write safety via file locking
  # ---------------------------------------------------------------------------

  # @req REQ-AUDIT-007
  @wip
  Scenario: Parallel sessions produce valid JSONL without corruption
    Given two aegis sessions running simultaneously
    When both sessions write entries to the same ledger directory
    Then each entry should be a complete valid JSON line
    And no partial writes or interleaved bytes should appear

  # @req REQ-AUDIT-007
  @wip
  Scenario: File lock prevents concurrent append corruption on POSIX
    Given two processes attempting to append simultaneously
    When flock is used for mutual exclusion
    Then each append should be atomic
    And the resulting file should be valid JSONL

  # ---------------------------------------------------------------------------
  # REQ-AUDIT-008: Crash recovery for truncated tail entries
  # ---------------------------------------------------------------------------

  # @req REQ-AUDIT-008
  @wip
  Scenario: Truncated tail entry is quarantined on recovery
    Given the ledger file ends with a partial JSON line (simulating a crash)
    When aegis starts and opens the ledger
    Then the partial bytes should be moved to a ".corrupt" sidecar file
    And a LEDGER_REPAIRED entry should be written as the first entry after recovery
    And subsequent writes should succeed normally

  # @req REQ-AUDIT-008
  @wip
  Scenario: Multiple corrupted tail bytes are handled
    Given the ledger file ends with 3 incomplete lines
    When aegis starts and opens the ledger
    Then all incomplete bytes should be quarantined
    And the repaired ledger should end with valid JSON lines only

  # ---------------------------------------------------------------------------
  # REQ-AUDIT-009: Rotated segments compressed with zstd
  # ---------------------------------------------------------------------------

  # @req REQ-AUDIT-009
  @wip
  Scenario: Closed ledger segment is compressed with zstd
    Given a ledger file has been rotated
    When compression runs on the closed segment
    Then the compressed file should have a ".jsonl.zst" extension
    And the compression level should be 3
    And the compressed file should be smaller than the original

  # @req REQ-AUDIT-009
  @wip
  Scenario: Active ledger file is never compressed
    Given the active ledger file is "~/.aegis/logs/current.jsonl"
    When I check the active file
    Then it should remain uncompressed JSONL
    And it should be directly appendable

  # ---------------------------------------------------------------------------
  # REQ-AUDIT-010: Retention policy purges segments beyond 90 days
  # ---------------------------------------------------------------------------

  # @req REQ-AUDIT-010
  @wip
  Scenario: Segments older than 90 days are purged on startup
    Given "~/.aegis/logs/" contains segments from 100 days ago
    When aegis starts
    Then segments older than 90 days should be deleted
    And a LEDGER_PURGED entry should be written with the count of purged files

  # @req REQ-AUDIT-010
  @wip
  Scenario: Custom retention_days is respected
    Given config contains "retention_days: 30"
    And "~/.aegis/logs/" contains segments from 45 days ago
    When aegis starts
    Then those segments should be purged

  # @req REQ-AUDIT-010
  @wip
  Scenario: Segments within retention window are preserved
    Given "~/.aegis/logs/" contains segments from 10 days ago
    When aegis starts
    Then those segments should remain untouched

  # ---------------------------------------------------------------------------
  # REQ-AUDIT-011: SIEM export formats
  # ---------------------------------------------------------------------------

  # @req REQ-AUDIT-011
  @wip
  Scenario: aegis audit export --format splunk produces valid HEC payload
    Given the ledger contains 50 entries
    When the user runs "aegis audit export --format splunk"
    Then the output should be valid Splunk HEC JSON
    And each entry should include time, host, source, and event fields

  # @req REQ-AUDIT-011
  @wip
  Scenario: aegis audit export --format elastic produces valid bulk API payload
    Given the ledger contains 50 entries
    When the user runs "aegis audit export --format elastic"
    Then the output should be valid Elasticsearch Bulk API NDJSON
    And each entry should have an index action followed by the document

  # @req REQ-AUDIT-011
  @wip
  Scenario: aegis audit export with --since and --until filters entries
    Given the ledger contains entries from the past 30 days
    When the user runs "aegis audit export --format splunk --since 2026-03-01 --until 2026-03-15"
    Then only entries within the date range should be included

  # ---------------------------------------------------------------------------
  # REQ-AUDIT-012: Real-time log forwarding
  # ---------------------------------------------------------------------------

  # @req REQ-AUDIT-012
  @wip
  Scenario: Entries are forwarded to HTTPS endpoint within 5 seconds
    Given config contains "log_forward_url: https://siem.corp.example/ingest"
    When a new audit entry is written
    Then the entry should be delivered to the HTTPS endpoint within 5 seconds

  # @req REQ-AUDIT-012
  @wip
  Scenario: Forwarding retries with backoff on transient failure
    Given the forwarding endpoint returns HTTP 503
    When an entry is written
    Then the forwarder should retry with exponential backoff
    And buffer entries up to 1000 while the endpoint is unavailable

  # @req REQ-AUDIT-012
  @wip
  Scenario: Forwarding failure does not block local ledger writes
    Given the forwarding endpoint is unreachable
    When entries are written to the local ledger
    Then local writes should succeed without delay
    And the forwarder should queue entries for later delivery

  # ---------------------------------------------------------------------------
  # REQ-AUDIT-013: Ledger search by event type, req_id, and time range
  # ---------------------------------------------------------------------------

  # @req REQ-AUDIT-013
  @wip
  Scenario: aegis audit search by event type returns matching entries
    Given the ledger contains HITL_APPROVED and HITL_DENIED entries
    When the user runs "aegis audit search --event-type HITL_DENIED"
    Then only HITL_DENIED entries should be returned

  # @req REQ-AUDIT-013
  @wip
  Scenario: aegis audit search by req_id returns matching entries
    Given the ledger contains entries with req_id "REQ-BUILD-001" and "REQ-TUI-001"
    When the user runs "aegis audit search --req-id REQ-BUILD-001"
    Then only entries with req_id "REQ-BUILD-001" should be returned

  # @req REQ-AUDIT-013
  @wip
  Scenario: Search spans compressed segments transparently
    Given the ledger has 5 compressed segments and 1 active segment
    When the user runs "aegis audit search --event-type SESSION_START"
    Then results should include entries from both compressed and active segments

  # ---------------------------------------------------------------------------
  # REQ-AUDIT-014: Compliance report ZIP bundle
  # ---------------------------------------------------------------------------

  # @req REQ-AUDIT-014
  @wip
  Scenario: aegis audit report produces structured evidence ZIP
    Given the ledger contains session data for the past 30 days
    When the user runs "aegis audit report --output evidence.zip"
    Then "evidence.zip" should contain "summary.json"
    And "events.jsonl" with filtered audit events
    And "integrity_check.json" with the hash chain verification result
    And "manifest.json" listing all files in the bundle

  # @req REQ-AUDIT-014
  @wip
  Scenario: Compliance report fails if integrity check fails
    Given the ledger has a tampered entry
    When the user runs "aegis audit report --output evidence.zip"
    Then the command should fail with "Cannot generate compliance report: ledger integrity check failed"
    And no ZIP file should be produced

  # ---------------------------------------------------------------------------
  # REQ-AUDIT-015: Redaction verification scan
  # ---------------------------------------------------------------------------

  # @req REQ-AUDIT-015
  @wip
  Scenario: aegis audit scan exits 0 when no CUI is found
    Given the ledger contains only metadata entries (no file contents or prompts)
    When the user runs "aegis audit scan"
    Then the command should exit with code 0
    And report "Scan complete: no CUI markers or PII patterns detected"

  # @req REQ-AUDIT-015
  @wip
  Scenario: aegis audit scan exits non-zero when SSN pattern is found
    Given the ledger contains an entry that somehow includes "123-45-6789"
    When the user runs "aegis audit scan"
    Then the command should exit with a non-zero code
    And report "PII detected: SSN pattern at entry N"

  # @req REQ-AUDIT-015
  @wip
  Scenario: aegis audit export is blocked when scan finds violations
    Given "aegis audit scan" reports CUI content in the ledger
    When the user runs "aegis audit export --format splunk"
    Then the export should be blocked
    And display "Export blocked: CUI scan failed. Run 'aegis audit scan' for details."

  # ---------------------------------------------------------------------------
  # REQ-AUDIT-016: NTP-sourced timestamps with drift detection
  # ---------------------------------------------------------------------------

  # @req REQ-AUDIT-016
  @wip
  Scenario: Timestamps include NTP offset and monotonic nanoseconds
    Given an aegis session with NTP available
    When an audit entry is written
    Then the entry should include "ntp_offset_ms" and "monotonic_ns" fields
    And the timestamp should be accurate within 1 second of NTP time

  # @req REQ-AUDIT-016
  @wip
  Scenario: Clock drift warning emitted when offset exceeds 5 seconds
    Given the system clock is 10 seconds behind NTP time
    When aegis writes an audit entry
    Then a CLOCK_DRIFT_WARNING entry should be recorded
    And the warning should include the measured offset

  # @req REQ-AUDIT-016
  @wip
  Scenario: Air-gapped mode skips NTP check
    Given aegis is in local/air-gapped mode
    When an audit entry is written
    Then the ntp_offset_ms field should be null
    And no NTP network call should be attempted

  # ---------------------------------------------------------------------------
  # REQ-AUDIT-017: Session reconstruction from ledger segments
  # ---------------------------------------------------------------------------

  # @req REQ-AUDIT-017
  @wip
  Scenario: aegis audit replay reconstructs full session timeline
    Given a session spanning 3 ledger segments (2 compressed, 1 active)
    When the user runs "aegis audit replay <session_id>"
    Then the output should display all events for that session in chronological order
    And events should be sorted by monotonic_ns

  # @req REQ-AUDIT-017
  @wip
  Scenario: Session reconstruction detects gaps in the timeline
    Given a session with a missing segment (data loss)
    When the user runs "aegis audit replay <session_id>"
    Then the output should indicate "Gap detected between entry N and entry M"
    And the reconstruction should continue with available data

  # @req REQ-AUDIT-017
  @wip
  Scenario: Session reconstruction works across segment boundaries
    Given a session that started before rotation and continued after
    When the user runs "aegis audit replay <session_id>"
    Then entries from both the pre-rotation and post-rotation segments should appear
    And the SESSION_CONTINUE boundary entry should be included
