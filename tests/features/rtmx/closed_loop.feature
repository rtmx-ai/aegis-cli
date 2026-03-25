Feature: RTMX Closed-Loop Verification
  As a defense engineer with compliance obligations
  I need the agent to link all work to RTMX requirements
  So that every code change has traceable evidence from requirement to test

  # ---------------------------------------------------------------------------
  # REQ-RTMX-001: Agent reads RTMX requirements from .rtmx/database.csv
  # ---------------------------------------------------------------------------

  # @req REQ-RTMX-001
  Scenario: Agent reads and parses requirements from RTMX corpus
    Given a workspace with ".rtmx/database.csv" containing REQ-AUTH-001
    When the user sends "Implement REQ-AUTH-001"
    Then the agent should invoke "read_file" on ".rtmx/database.csv"
    And parse the CSV per the RTMX schema
    And understand the requirement_text, target_value, and acceptance criteria

  # @req REQ-RTMX-001
  Scenario: Agent reports error when .rtmx/database.csv is missing
    Given a workspace without an ".rtmx/database.csv" file
    When the user sends "Implement REQ-AUTH-001"
    Then the agent should report "RTMX database not found at .rtmx/database.csv"
    And suggest running "rtmx init" to create the database

  # @req REQ-RTMX-001
  Scenario: Agent reads a specific requirement by ID via read_requirement tool
    Given ".rtmx/database.csv" contains 50 requirements
    When the agent invokes "read_requirement" with req_id "REQ-BUILD-003"
    Then the tool should return only the row for REQ-BUILD-003
    And include all columns: req_id, category, requirement_text, target_value, status

  # ---------------------------------------------------------------------------
  # REQ-RTMX-002: Agent updates requirement status and test results
  # ---------------------------------------------------------------------------

  # @req REQ-RTMX-002
  Scenario: Agent updates requirement status after successful verification
    Given the agent has implemented code for REQ-AUTH-001
    And all linked tests pass
    When the agent updates ".rtmx/database.csv"
    Then REQ-AUTH-001 status should change from "TODO" to "COMPLETE"
    And the test_module and test_function columns should be populated
    And the completed_date should be set to today's date

  # @req REQ-RTMX-002
  Scenario: Agent update requires HITL approval before writing
    Given the agent proposes updating REQ-AUTH-001 status to "COMPLETE"
    When the HITL gate activates for the write to ".rtmx/database.csv"
    Then the user should see the proposed changes in a diff
    And the CSV should not be modified until the user approves

  # @req REQ-RTMX-002
  Scenario: Agent preserves CSV schema when updating rows
    Given ".rtmx/database.csv" has 19 columns per the RTMX schema
    When the agent updates REQ-AUTH-001
    Then the updated CSV should still have exactly 19 columns per row
    And no columns should be reordered, added, or removed
    And all other rows should remain unchanged

  # ---------------------------------------------------------------------------
  # REQ-RTMX-003: Closed-loop verification: requirement -> test -> evidence
  # ---------------------------------------------------------------------------

  # @req REQ-RTMX-003
  Scenario: Agent refuses to mark requirement complete without passing tests
    Given the agent has implemented code for REQ-AUTH-001
    But the linked test "tests/auth/auth_test.rs::test_auth_flow" is failing
    When the agent attempts to mark REQ-AUTH-001 as "COMPLETE"
    Then the status should remain "TODO"
    And the agent should report which tests are failing
    And the agent should enter a fix loop to address the failures

  # @req REQ-RTMX-003
  Scenario: aegis verify runs linked tests and seals evidence on success
    Given REQ-AUTH-001 has linked test_module and test_function
    When the user runs "aegis verify REQ-AUTH-001"
    Then aegis should execute the linked tests
    And if all pass, status should update to "COMPLETE"
    And evidence should be sealed in the audit ledger with the test output hash

  # @req REQ-RTMX-003
  Scenario: aegis verify fails and leaves status unchanged on test failure
    Given REQ-AUTH-001 has linked tests that are failing
    When the user runs "aegis verify REQ-AUTH-001"
    Then the status should remain "TODO"
    And the failing test names and error messages should be displayed
    And the audit ledger should record "VERIFICATION_FAILED" with test details

  # ---------------------------------------------------------------------------
  # REQ-RTMX-004: Test marker scanning for Rust test files
  # ---------------------------------------------------------------------------

  # @req REQ-RTMX-004
  Scenario: Marker scanning discovers // @req comments in test files
    Given a test file "tests/auth/auth_test.rs" contains "// @req REQ-AUTH-001"
    When the marker scanner runs
    Then REQ-AUTH-001 should be linked to test_module "tests/auth/auth_test.rs"
    And the test_function should be populated from the nearest #[test] attribute

  # @req REQ-RTMX-004
  Scenario: Marker scanning discovers #[req()] attribute macros
    Given a test file contains "#[req(REQ-BUILD-003)]" above a test function
    When the marker scanner runs
    Then REQ-BUILD-003 should be linked to the annotated test function

  # @req REQ-RTMX-004
  Scenario: Marker scanner reports orphaned markers
    Given a test file contains "// @req REQ-NONEXISTENT-999"
    And "REQ-NONEXISTENT-999" does not exist in ".rtmx/database.csv"
    When the marker scanner runs
    Then a warning should be emitted: "Orphaned marker: REQ-NONEXISTENT-999 not found in database"

  # ---------------------------------------------------------------------------
  # REQ-RTMX-005: NIST 800-171 control identifiers on every requirement
  # ---------------------------------------------------------------------------

  # @req REQ-RTMX-005
  Scenario: All requirements have non-empty nist_controls column
    Given ".rtmx/database.csv" contains 100 requirements
    When the schema validator runs
    Then every row should have a non-empty "nist_controls" column
    And each value should contain comma-separated control IDs matching pattern "3.x.x"

  # @req REQ-RTMX-005
  Scenario: Schema validator rejects requirement with missing nist_controls
    Given a requirement row with an empty "nist_controls" field
    When the schema validator runs
    Then a validation error should be reported for that row
    And the error should indicate "nist_controls is required"

  # ---------------------------------------------------------------------------
  # REQ-RTMX-006: Dependency graph visualization and cycle detection
  # ---------------------------------------------------------------------------

  # @req REQ-RTMX-006
  Scenario: aegis rtmx graph renders a DAG in DOT format
    Given ".rtmx/database.csv" contains requirements with dependencies
    When the user runs "aegis rtmx graph --format dot"
    Then the output should be valid DOT graph language
    And each node should represent a requirement ID
    And each edge should represent a dependency

  # @req REQ-RTMX-006
  Scenario: Cycle in dependency graph is detected and reported
    Given REQ-A depends on REQ-B and REQ-B depends on REQ-A
    When the user runs "aegis rtmx graph"
    Then the command should report "Dependency cycle detected: REQ-A -> REQ-B -> REQ-A"
    And exit with a non-zero code

  # @req REQ-RTMX-006
  Scenario: aegis rtmx graph renders mermaid format
    Given ".rtmx/database.csv" contains requirements with dependencies
    When the user runs "aegis rtmx graph --format mermaid"
    Then the output should be valid Mermaid graph syntax

  # ---------------------------------------------------------------------------
  # REQ-RTMX-007: Requirement prioritization and critical-path analysis
  # ---------------------------------------------------------------------------

  # @req REQ-RTMX-007
  Scenario: aegis rtmx plan emits ordered implementation plan
    Given ".rtmx/database.csv" contains requirements with priorities and effort_weeks
    When the user runs "aegis rtmx plan"
    Then the output should list requirements in topological order
    And requirements with no unresolved dependencies should appear first
    And the critical path should be highlighted

  # @req REQ-RTMX-007
  Scenario: Plan weights by priority and effort
    Given REQ-A has priority CRITICAL and effort 1 week
    And REQ-B has priority LOW and effort 3 weeks
    And both have no dependencies
    When the user runs "aegis rtmx plan"
    Then REQ-A should appear before REQ-B

  # ---------------------------------------------------------------------------
  # REQ-RTMX-008: Requirement conflict detection
  # ---------------------------------------------------------------------------

  # @req REQ-RTMX-008
  Scenario: Conflicting target_values between requirements are detected
    Given REQ-X specifies target_value "TLS 1.2 minimum"
    And REQ-Y specifies target_value "TLS 1.3 required, no TLS 1.2"
    When the conflict detector runs
    Then a WARNING should be reported: "Potential conflict: REQ-X and REQ-Y have contradictory TLS targets"

  # @req REQ-RTMX-008
  Scenario: Conflict detection does not block reads
    Given a conflict is detected between two requirements
    When the agent reads requirements from the database
    Then the read should succeed
    And the conflict should be reported as a warning, not an error

  # ---------------------------------------------------------------------------
  # REQ-RTMX-009: CRDT-based multiplayer RTMX sync
  # ---------------------------------------------------------------------------

  # @req REQ-RTMX-009
  Scenario: Concurrent edits to different requirements converge
    Given two users edit REQ-A and REQ-B simultaneously
    When both changes are synced via Automerge CRDT
    Then the merged database should contain both updates
    And no data should be lost from either edit

  # @req REQ-RTMX-009
  Scenario: Concurrent edits to the same requirement field resolve
    Given two users update REQ-A status simultaneously (one to "IN_PROGRESS", one to "COMPLETE")
    When CRDT merge occurs
    Then the last-writer-wins field should resolve deterministically
    And a merge conflict notice should be logged in the audit ledger

  # ---------------------------------------------------------------------------
  # REQ-RTMX-010: Requirement import from DOORS / JIRA / OSCAL
  # ---------------------------------------------------------------------------

  # @req REQ-RTMX-010
  Scenario: aegis rtmx import converts OSCAL JSON to database.csv format
    Given an OSCAL catalog JSON file "nist-800-171-oscal.json"
    When the user runs "aegis rtmx import --format oscal --file nist-800-171-oscal.json"
    Then ".rtmx/database.csv" should contain new rows for each imported control
    And the nist_controls column should be populated from the OSCAL control IDs
    And existing rows should not be modified

  # @req REQ-RTMX-010
  Scenario: Import is idempotent for duplicate requirement IDs
    Given ".rtmx/database.csv" already contains REQ-IMPORTED-001
    When the user imports a file that also contains REQ-IMPORTED-001
    Then the existing row should not be duplicated
    And the import should report "Skipped 1 duplicate requirement(s)"

  # @req REQ-RTMX-010
  Scenario: aegis rtmx import converts JIRA CSV export to database.csv format
    Given a JIRA CSV export file "jira-export.csv"
    When the user runs "aegis rtmx import --format jira --file jira-export.csv"
    Then new rows should be added to ".rtmx/database.csv"
    And the requirement_text should be populated from the JIRA Summary field
