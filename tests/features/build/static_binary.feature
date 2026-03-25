Feature: Static Binary Build
  As a defense engineer deploying to NIPR/SIPR
  I need aegis to be a single static binary with no runtime dependencies
  So that I can transfer and install it on locked-down government workstations

  # @req REQ-BUILD-001
  Scenario: Binary has no dynamic library dependencies on Linux
    Given the aegis binary is built for x86_64-unknown-linux-musl
    When I inspect the binary with ldd
    Then it should report "not a dynamic executable"
    And the binary should be less than 100MB

  # @req REQ-BUILD-001
  Scenario: Binary runs on RHEL without runtime dependencies
    Given a clean RHEL 8 container with no development tools
    When I copy the aegis binary into the container
    And I execute "aegis --version"
    Then it should print the version string and exit 0

  # @req REQ-BUILD-002
  Scenario: Standalone installer works offline
    Given the aegis RPM package
    When I install it on a RHEL system with no network access
    Then the installation should succeed
    And "aegis --version" should work

  # @req REQ-BUILD-003
  Scenario: Binary is signed and has SBOM
    Given a release build of aegis
    Then the binary should have a valid GPG signature
    And a CycloneDX SBOM should exist alongside the binary
