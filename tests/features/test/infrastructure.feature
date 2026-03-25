Feature: Test Infrastructure and Quality Gates
  As a defense engineer maintaining aegis quality
  I need deterministic, isolated, and comprehensive test infrastructure
  So that every CI run produces reliable results and meets coverage thresholds

  # ---------------------------------------------------------------------------
  # REQ-TEST-001: All tests fully deterministic with no shared mutable state
  # ---------------------------------------------------------------------------

  # @req REQ-TEST-001
  Scenario: 100 consecutive test runs produce identical results
    Given the test suite is executed with a fixed PROPTEST_SEED
    When I run "cargo nextest run" 100 times
    Then every run should produce the same pass/fail result for every test
    And no test should be flaky

  # @req REQ-TEST-001
  Scenario: Each test uses a unique TempDir with no shared state
    Given any test that creates files
    When the test creates its working directory
    Then it should use a unique TempDir allocated for that test only
    And the TempDir should be cleaned up after the test completes
    And no global mutable state should exist between tests

  # @req REQ-TEST-001
  Scenario: Tests do not depend on execution order
    Given the full test suite
    When I run tests in reverse alphabetical order
    Then all tests should still pass
    And no test should depend on side effects from another test

  # ---------------------------------------------------------------------------
  # REQ-TEST-002: LLM record/replay infrastructure for deterministic tests
  # ---------------------------------------------------------------------------

  # @req REQ-TEST-002
  Scenario: LLM cassette records a Vertex AI interaction
    Given AEGIS_RECORD_CASSETTES=1 is set in the environment
    And a valid Vertex AI provider is configured
    When a test sends a conversation to the LLM
    Then a cassette file should be written to "tests/fixtures/cassettes/<test_name>.json"
    And it should contain the request and response payloads

  # @req REQ-TEST-002
  Scenario: LLM cassette replays recorded interaction deterministically
    Given a cassette file exists at "tests/fixtures/cassettes/vertex_streaming_001.json"
    And AEGIS_RECORD_CASSETTES is not set
    When the test replays the cassette
    Then the replayed response should be byte-for-byte identical to the recording
    And no network connection should be made during replay

  # @req REQ-TEST-002
  Scenario: Test fails if cassette is missing and recording is disabled
    Given no cassette file exists for a test
    And AEGIS_RECORD_CASSETTES is not set
    When the test attempts to replay
    Then the test should fail with "Cassette not found: tests/fixtures/cassettes/<name>.json"
    And no live API call should be made

  # ---------------------------------------------------------------------------
  # REQ-TEST-003: TUI snapshot testing via ratatui TestBackend + insta
  # ---------------------------------------------------------------------------

  # @req REQ-TEST-003
  Scenario: TUI widget has a golden snapshot
    Given a ratatui TestBackend of 80x24
    When the chat layout widget is rendered with sample data
    Then the rendered buffer should match the golden snapshot in "tests/snapshots/"
    And "cargo insta review" should show no pending changes

  # @req REQ-TEST-003
  Scenario: Snapshot update workflow via cargo insta review
    Given a TUI widget rendering has changed intentionally
    When I run "cargo insta review"
    Then the tool should display the diff between old and new snapshot
    And accepting the change should update the golden file in "tests/snapshots/"

  # @req REQ-TEST-003
  Scenario: CI fails when snapshot does not match golden file
    Given a TUI widget rendering differs from the golden snapshot
    When the CI pipeline runs "cargo nextest run"
    Then the snapshot test should fail
    And the error should indicate which snapshot file differs

  # ---------------------------------------------------------------------------
  # REQ-TEST-004: Minimum 80% line and 70% branch coverage enforced in CI
  # ---------------------------------------------------------------------------

  # @req REQ-TEST-004
  Scenario: CI passes when coverage thresholds are met
    Given the test suite achieves 85% line coverage and 75% branch coverage
    When the coverage gate runs via "cargo llvm-cov"
    Then the gate should pass
    And the coverage report should be generated

  # @req REQ-TEST-004
  Scenario: CI fails when line coverage drops below 80%
    Given the test suite achieves 78% line coverage
    When the coverage gate runs
    Then the gate should fail with "Line coverage 78% is below minimum 80%"

  # @req REQ-TEST-004
  Scenario: CI fails when branch coverage drops below 70%
    Given the test suite achieves 68% branch coverage
    When the coverage gate runs
    Then the gate should fail with "Branch coverage 68% is below minimum 70%"

  # @req REQ-TEST-004
  Scenario: Coverage exclusions use only the approved attribute
    Given a function annotated with #[cfg(not(coverage))]
    When coverage is computed
    Then the annotated function should be excluded from coverage metrics
    And no other exclusion mechanism should be used

  # ---------------------------------------------------------------------------
  # REQ-TEST-005: All three test tiers required before PR merge
  # ---------------------------------------------------------------------------

  # @req REQ-TEST-005
  Scenario: Branch protection requires unit, integration, and E2E checks
    Given a PR is opened against "main"
    When the CI pipeline runs
    Then unit tests, integration tests, and E2E tests should all execute
    And the PR should not be mergeable until all three tiers pass

  # @req REQ-TEST-005
  Scenario: Merge queue is enforced and --no-verify bypass is blocked
    Given a developer attempts "git push --no-verify"
    When the push reaches the remote
    Then the branch protection rules should still apply
    And the merge queue should require all CI checks to pass

  # @req REQ-TEST-005
  Scenario: PR with failing integration tests cannot be merged
    Given a PR where unit tests pass but integration tests fail
    When the developer attempts to merge
    Then the merge should be blocked
    And the failing integration test names should be visible in the PR checks

  # ---------------------------------------------------------------------------
  # REQ-TEST-006: No shared filesystem or network state between tests
  # ---------------------------------------------------------------------------

  # @req REQ-TEST-006
  Scenario: Parallel nextest produces no order-dependent failures
    Given the full test suite running with "cargo nextest run -j 8"
    When 8 tests execute in parallel
    Then no test should fail due to contention with another test
    And no test should reference "~/.aegis" directly

  # @req REQ-TEST-006
  Scenario: Each test uses wiremock on port 0 for unique server binding
    Given a test that needs an HTTP mock server
    When the test starts a wiremock server
    Then it should bind to port 0 (OS-assigned ephemeral port)
    And no two tests should share the same mock server

  # @req REQ-TEST-006
  Scenario: Tests do not access real ~/.aegis directory
    Given a test that needs aegis configuration
    When the test sets up its environment
    Then it should use a test-specific TempDir as the aegis home
    And the real "~/.aegis" directory should never be read or written

  # ---------------------------------------------------------------------------
  # REQ-TEST-007: Structured fixture management via typed factory functions
  # ---------------------------------------------------------------------------

  # @req REQ-TEST-007
  Scenario: Fixture factory produces valid RTMX database
    Given a test needs a sample ".rtmx/database.csv"
    When rtmx_database_fixture() is called
    Then it should return a valid CSV file in a TempDir
    And the CSV should conform to the RTMX schema with all required columns

  # @req REQ-TEST-007
  Scenario: Fixture factory produces valid aegis config
    Given a test needs a sample "config.yaml"
    When config_fixture(mode: "local") is called
    Then it should return a valid config file with permissions 0600
    And the mode field should be "local"
    And all required fields should be present

  # @req REQ-TEST-007
  Scenario: Fixture factory produces valid LLM cassette
    Given a test needs a recorded LLM interaction
    When cassette_fixture("simple_chat") is called
    Then it should return a valid cassette JSON file
    And the cassette should contain at least one request/response pair

  # ---------------------------------------------------------------------------
  # REQ-TEST-008: All BDD scenarios have corresponding Cucumber step definitions
  # ---------------------------------------------------------------------------

  # @req REQ-TEST-008
  Scenario: CI reports zero undefined Cucumber steps
    Given the BDD feature files in "tests/features/"
    When "cargo test --test cucumber" runs
    Then the report should show 0 undefined steps
    And 0 pending steps

  # @req REQ-TEST-008
  Scenario: CI fails when a new scenario lacks step definitions
    Given a new scenario is added to a feature file without step implementations
    When the CI pipeline runs
    Then the Cucumber test runner should report undefined steps
    And the CI job should fail

  # @req REQ-TEST-008
  Scenario: Step definitions match scenarios by regex patterns
    Given the step definition "the agent invokes {string} on {string}"
    When a scenario contains 'the agent invokes "read_file" on "src/main.rs"'
    Then the step should match and execute the corresponding test code

  # ---------------------------------------------------------------------------
  # REQ-TEST-009: Property-based tests cover all CSV and config parsers
  # ---------------------------------------------------------------------------

  # @req REQ-TEST-009
  Scenario: CSV parser does not panic on arbitrary UTF-8 input
    Given proptest generates 10000 arbitrary UTF-8 strings
    When each string is fed to the RTMX CSV parser
    Then the parser should either return a valid result or an error
    And it should never panic

  # @req REQ-TEST-009
  Scenario: YAML config parser does not panic on arbitrary input
    Given proptest generates 10000 arbitrary byte sequences
    When each sequence is fed to the config YAML parser
    Then the parser should either return a valid result or an error
    And it should never panic

  # @req REQ-TEST-009
  Scenario: .aegisignore parser does not panic on arbitrary patterns
    Given proptest generates 10000 arbitrary glob patterns
    When each pattern is fed to the .aegisignore parser
    Then the parser should either compile the pattern or return an error
    And it should never panic

  # @req REQ-TEST-009
  Scenario: Property tests use fixed seed in CI for reproducibility
    Given PROPTEST_SEED is set to a fixed value in CI
    When property-based tests run
    Then the same test cases should be generated every run

  # ---------------------------------------------------------------------------
  # REQ-TEST-010: Performance benchmarks with regression gates
  # ---------------------------------------------------------------------------

  # @req REQ-TEST-010
  Scenario: Agent loop iteration overhead is under 500 microseconds
    Given the benchmark harness is configured with criterion
    When "cargo bench --bench agent_loop" runs
    Then the median iteration time should be under 500 microseconds

  # @req REQ-TEST-010
  Scenario: TUI render cycle completes under 16 milliseconds
    Given a TUI render benchmark with sample chat data
    When "cargo bench --bench tui_render" runs
    Then the median render time should be under 16 milliseconds (60 FPS budget)

  # @req REQ-TEST-010
  Scenario: CI fails when benchmark regresses by more than 25%
    Given a baseline benchmark result exists
    When the current benchmark shows a 30% regression
    Then the CI benchmark gate should fail
    And the regression report should identify the slower benchmark by name

  # ---------------------------------------------------------------------------
  # REQ-TEST-011: Cross-platform test matrix for RHEL and Windows
  # ---------------------------------------------------------------------------

  # @req REQ-TEST-011
  Scenario: All tests pass on musl target
    Given the CI matrix includes "x86_64-unknown-linux-musl"
    When "cargo nextest run" executes on the musl target
    Then all tests should pass

  # @req REQ-TEST-011
  Scenario: All tests pass on MSVC target
    Given the CI matrix includes "x86_64-pc-windows-msvc"
    When "cargo nextest run" executes on the MSVC target
    Then all tests should pass

  # @req REQ-TEST-011
  Scenario: Platform-specific tests use cfg guards
    Given a test that checks POSIX file permissions
    When the test is compiled on Windows
    Then it should be gated behind #[cfg(unix)]
    And a Windows-specific equivalent should be gated behind #[cfg(windows)]

  # @req REQ-TEST-011
  Scenario: Path normalization works across platforms
    Given a test that constructs file paths
    When the test runs on both Linux and Windows
    Then path separators should be normalized correctly
    And no test should hard-code "/" or "\" path separators

  # ---------------------------------------------------------------------------
  # REQ-TEST-012: Time control via tokio pause; no wall-clock sleeps in tests
  # ---------------------------------------------------------------------------

  # @req REQ-TEST-012
  Scenario: Timeout tests use tokio::time::pause instead of real sleep
    Given a test that verifies a 60-second timeout
    When the test calls tokio::time::pause()
    Then tokio::time::advance(Duration::from_secs(60)) should trigger the timeout instantly
    And the test should complete in milliseconds, not 60 seconds

  # @req REQ-TEST-012
  Scenario: CI grep gate rejects thread::sleep in test code
    Given the test source files in "tests/"
    When CI scans for "thread::sleep" in test code
    Then no occurrences should be found
    And the gate should pass with exit code 0

  # @req REQ-TEST-012
  Scenario: CI grep gate rejects std::thread::sleep in test code
    Given the test source files in "tests/"
    When CI scans for "std::thread::sleep" in test code
    Then no occurrences should be found

  # @req REQ-TEST-012
  Scenario: Time-dependent test produces deterministic result
    Given a test that checks HITL approval timeout behavior
    When tokio::time::pause() is active
    And the test advances time by exactly 60 seconds
    Then the timeout should fire deterministically
    And the test result should be identical across 100 runs
