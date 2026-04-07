Feature: Static Binary Build Pipeline
  As a defense engineer deploying aegis to classified networks
  I need a statically linked, signed, and reproducible binary
  So that it runs on RHEL and Windows without runtime dependencies and passes STIG review

  # ---------------------------------------------------------------------------
  # REQ-BUILD-001: Static binary for x86_64 Linux (RHEL) and Windows
  # ---------------------------------------------------------------------------

  # @req REQ-BUILD-001
  Scenario: Linux musl binary has no dynamic library dependencies
    Given the CI pipeline has built "aegis" for target "x86_64-unknown-linux-musl"
    When I run "ldd target/x86_64-unknown-linux-musl/release/aegis"
    Then the output should contain "not a dynamic executable"
    And the binary should execute on a bare RHEL 8 container without installing packages

  # @req REQ-BUILD-001
  Scenario: Linux musl binary fails gracefully on unsupported architecture
    Given the CI pipeline has built "aegis" for target "x86_64-unknown-linux-musl"
    When I attempt to run the binary on an aarch64 Linux host
    Then the OS should report "cannot execute binary file: Exec format error"
    And the exit code should be non-zero

  # @req REQ-BUILD-001
  Scenario: Windows binary runs on Win10 without runtime dependencies
    Given the CI pipeline has built "aegis.exe" for target "x86_64-pc-windows-msvc"
    When I run "aegis.exe --version" on a clean Windows 10 installation
    Then the binary should print the version string
    And no "missing DLL" error should appear

  # @req REQ-BUILD-001
  Scenario: Windows binary runs on Win11 without runtime dependencies
    Given the CI pipeline has built "aegis.exe" for target "x86_64-pc-windows-msvc"
    When I run "aegis.exe --version" on a clean Windows 11 installation
    Then the binary should print the version string
    And no "missing DLL" error should appear

  # ---------------------------------------------------------------------------
  # REQ-BUILD-002: Standalone installer packaging for closed network transfer
  # ---------------------------------------------------------------------------

  # @req REQ-BUILD-002
  Scenario: RPM installer works offline on RHEL 9
    Given the RPM package "aegis-0.1.0-1.x86_64.rpm" has been built
    When I run "rpm -ivh aegis-0.1.0-1.x86_64.rpm" on a RHEL 9 host with no network
    Then the package should install successfully
    And "aegis --version" should print the correct version
    And no network calls should be made during installation

  # @req REQ-BUILD-002
  Scenario: DEB installer works offline on Debian-based systems
    Given the DEB package "aegis_0.1.0_amd64.deb" has been built
    When I run "dpkg -i aegis_0.1.0_amd64.deb" on an Ubuntu 22.04 host with no network
    Then the package should install successfully
    And "aegis --version" should print the correct version

  # @req REQ-BUILD-002
  Scenario: MSI installer works offline on Windows
    Given the MSI package "aegis-0.1.0-x64.msi" has been built
    When I run "msiexec /i aegis-0.1.0-x64.msi /qn" on a Windows host with no network
    Then the installation should complete with exit code 0
    And "aegis.exe --version" should print the correct version

  # @req REQ-BUILD-002
  Scenario: Installer is rejected if binary signature is invalid
    Given the RPM package "aegis-0.1.0-1.x86_64.rpm" has a corrupted GPG signature
    When I run "rpm -K aegis-0.1.0-1.x86_64.rpm"
    Then the output should indicate "SIGNATURES NOT OK"
    And installation should be blocked by default RPM policy

  # ---------------------------------------------------------------------------
  # REQ-BUILD-003: Binary signing and SBOM generation
  # ---------------------------------------------------------------------------

  # @req REQ-BUILD-003
  Scenario: Linux binary has valid GPG signature
    Given the CI pipeline has produced "aegis" and "aegis.sig"
    When I verify "gpg --verify aegis.sig aegis"
    Then the verification should succeed with "Good signature"
    And the signing key fingerprint should match the project release key

  # @req REQ-BUILD-003
  Scenario: SBOM is generated in CycloneDX format
    Given the CI pipeline has completed a release build
    When I inspect "aegis.cdx.json"
    Then it should be valid CycloneDX JSON
    And it should list all Cargo dependencies with exact versions
    And it should include license information for each component

  # @req REQ-BUILD-003
  Scenario: Windows binary has valid Authenticode signature
    Given the CI pipeline has produced "aegis.exe" with Authenticode signing
    When I run "signtool verify /pa aegis.exe" on Windows
    Then the verification should report "Successfully verified"

  # @req REQ-BUILD-003
  Scenario: SBOM generation fails if Cargo.lock is missing
    Given the repository does not contain a "Cargo.lock" file
    When the SBOM generation step runs
    Then the CI step should fail with a non-zero exit code
    And the error should indicate "Cargo.lock required for SBOM generation"

  # ---------------------------------------------------------------------------
  # REQ-BUILD-004: Cross-compilation producing Linux musl and Windows MSVC
  # ---------------------------------------------------------------------------

  # @req REQ-BUILD-004
  Scenario: CI pipeline produces both Linux and Windows binaries in one run
    Given the CI pipeline triggers on a push to "main"
    When the cross-compilation job completes
    Then artifact "aegis" for "x86_64-unknown-linux-musl" should exist
    And artifact "aegis.exe" for "x86_64-pc-windows-msvc" should exist
    And both artifacts should be attached to the pipeline run

  # @req REQ-BUILD-004
  Scenario: Cross-compilation fails cleanly when toolchain is misconfigured
    Given the CI pipeline runs without the "x86_64-unknown-linux-musl" target installed
    When the cross-compilation step executes
    Then it should fail with a descriptive error mentioning the missing target
    And the pipeline should not produce partial artifacts

  # ---------------------------------------------------------------------------
  # REQ-BUILD-005: Reproducible builds producing identical binaries
  # ---------------------------------------------------------------------------

  # @req REQ-BUILD-005
  Scenario: Two independent builds produce identical SHA-256 hashes
    Given SOURCE_DATE_EPOCH is set to "1711324800"
    And Cargo.lock is committed and rust-toolchain.toml pins the toolchain
    When I build "aegis" twice from the same commit in separate clean environments
    Then the SHA-256 hash of both binaries should be identical

  # @req REQ-BUILD-005
  Scenario: Reproducible build fails if Cargo.lock is modified between builds
    Given two builds from the same commit
    But Cargo.lock has an updated dependency between the builds
    When I compare the SHA-256 hashes
    Then the hashes should differ
    And CI should flag the non-reproducibility

  # ---------------------------------------------------------------------------
  # REQ-BUILD-006: cargo-deny enforces license allowlist and blocks vulnerabilities
  # ---------------------------------------------------------------------------

  # @req REQ-BUILD-006
  Scenario: cargo-deny passes with only allowed licenses
    Given all dependencies use Apache-2.0, MIT, or BSD licenses
    When I run "cargo deny check licenses"
    Then the command should exit with code 0
    And no license violations should be reported

  # @req REQ-BUILD-006
  Scenario: cargo-deny blocks GPL-licensed dependency
    Given a dependency with license "GPL-3.0" is added to Cargo.toml
    When I run "cargo deny check licenses"
    Then the command should exit with a non-zero code
    And the output should identify the GPL-licensed crate by name

  # @req REQ-BUILD-006
  Scenario: cargo-deny blocks crate with known GHSA advisory
    Given a dependency has a known GHSA advisory in the rustsec advisory database
    When I run "cargo deny check advisories"
    Then the command should exit with a non-zero code
    And the output should reference the GHSA identifier

  # ---------------------------------------------------------------------------
  # REQ-BUILD-007: Release binary stripped and LTO-optimized to minimum size
  # ---------------------------------------------------------------------------

  # @req REQ-BUILD-007
  Scenario: Linux musl release binary is under 20 MB
    Given the CI pipeline has built "aegis" for "x86_64-unknown-linux-musl" in release mode
    When I check the file size of the binary
    Then it should be less than 20971520 bytes
    And "file aegis" should indicate "stripped"

  # @req REQ-BUILD-007
  Scenario: Windows release binary is under 25 MB
    Given the CI pipeline has built "aegis.exe" for "x86_64-pc-windows-msvc" in release mode
    When I check the file size of the binary
    Then it should be less than 26214400 bytes

  # @req REQ-BUILD-007
  Scenario: Debug symbols are not present in release binary
    Given the release binary "aegis" for Linux musl
    When I run "nm aegis" or "readelf --debug-dump aegis"
    Then no debug symbol sections should be present

  # ---------------------------------------------------------------------------
  # REQ-BUILD-008: Binary links FIPS 140-2 validated crypto primitives
  # ---------------------------------------------------------------------------

  # @req REQ-BUILD-008
  Scenario: Binary passes FIPS self-test on RHEL in FIPS mode
    Given a RHEL 9 host with FIPS mode enabled via "fips-mode-setup --enable"
    When I run "aegis --fips-self-test"
    Then the output should contain "FIPS 140-2 self-test: PASSED"
    And the exit code should be 0

  # @req REQ-BUILD-008
  Scenario: Binary refuses TLS 1.2 when FIPS mode enforces TLS 1.3
    Given FIPS mode is enabled on the host
    And the configured endpoint only supports TLS 1.2
    When I run "aegis chat"
    Then aegis should refuse the connection
    And display "TLS version below minimum required by FIPS policy"

  # @req REQ-BUILD-008
  Scenario: FIPS self-test fails when crypto module is corrupted
    Given the FIPS crypto module integrity check data has been tampered with
    When I run "aegis --fips-self-test"
    Then the output should contain "FIPS 140-2 self-test: FAILED"
    And the exit code should be non-zero

  # ---------------------------------------------------------------------------
  # REQ-BUILD-009: Windows MSI installer via WiX for enterprise deployment
  # ---------------------------------------------------------------------------

  # @req REQ-BUILD-009
  Scenario: MSI installs silently via msiexec for SCCM deployment
    Given the MSI package "aegis-0.1.0-x64.msi" is Authenticode-signed
    When I run "msiexec /i aegis-0.1.0-x64.msi /qn"
    Then the installation should complete with exit code 0
    And "C:\Program Files\aegis\aegis.exe" should exist
    And the PATH environment variable should include the aegis directory

  # @req REQ-BUILD-009
  Scenario: MSI uninstalls cleanly without residual files
    Given aegis was installed via MSI
    When I run "msiexec /x aegis-0.1.0-x64.msi /qn"
    Then "C:\Program Files\aegis\aegis.exe" should not exist
    And the uninstall should complete with exit code 0

  # @req REQ-BUILD-009
  Scenario: MSI rejects installation if Authenticode signature is invalid
    Given the MSI package has a corrupted Authenticode signature
    When I attempt to install via "msiexec /i aegis-0.1.0-x64.msi /qn"
    Then the installation should fail
    And Windows should report a signature verification error

  # ---------------------------------------------------------------------------
  # REQ-BUILD-010: Linux RPM/DEB with correct ownership and SELinux labels
  # ---------------------------------------------------------------------------

  # @req REQ-BUILD-010
  Scenario: RPM installs on RHEL 9 with correct SELinux type
    Given the RPM package "aegis-0.1.0-1.x86_64.rpm" is GPG-signed
    When I install the RPM on a RHEL 9 host with SELinux enforcing
    Then "ls -Z /usr/local/bin/aegis" should show SELinux type "bin_t"
    And the binary owner should be "root:root"
    And the binary permissions should be 0755

  # @req REQ-BUILD-010
  Scenario: DEB installs with correct file ownership
    Given the DEB package "aegis_0.1.0_amd64.deb" has been built
    When I install the DEB on an Ubuntu 22.04 host
    Then the binary at "/usr/local/bin/aegis" should be owned by "root:root"
    And the binary permissions should be 0755

  # @req REQ-BUILD-010
  Scenario: RPM GPG signature verification succeeds
    Given the RPM package is signed with the project GPG key
    When I run "rpm -K aegis-0.1.0-1.x86_64.rpm"
    Then the output should contain "digests signatures OK"

  # ---------------------------------------------------------------------------
  # REQ-BUILD-011: Closed-network update bundle for offline version upgrades
  # ---------------------------------------------------------------------------

  # @req REQ-BUILD-011
  Scenario: Airgap update bundle installs newer version without network
    Given aegis version "0.1.0" is installed
    And the update bundle "aegis-update-0.2.0.tar.gz" contains the binary, SHA-256 manifest, and GPG signature
    When I run "aegis update --bundle aegis-update-0.2.0.tar.gz"
    Then "aegis --version" should print "0.2.0"
    And no network calls should be made during the update

  # @req REQ-BUILD-011
  Scenario: Airgap update rejects bundle with mismatched SHA-256
    Given the update bundle "aegis-update-0.2.0.tar.gz" has a corrupted binary
    And the SHA-256 in the manifest does not match the binary
    When I run "aegis update --bundle aegis-update-0.2.0.tar.gz"
    Then the update should fail with "SHA-256 verification failed"
    And the existing installation should remain at version "0.1.0"

  # @req REQ-BUILD-011
  Scenario: Airgap update rejects bundle with invalid GPG signature
    Given the update bundle has an invalid GPG signature
    When I run "aegis update --bundle aegis-update-0.2.0.tar.gz"
    Then the update should fail with "GPG signature verification failed"
    And the existing installation should remain unchanged

  # ---------------------------------------------------------------------------
  # REQ-BUILD-012: Git SHA, build date, and target triple embedded at compile
  # ---------------------------------------------------------------------------

  # @req REQ-BUILD-012
  Scenario: aegis --version prints git SHA and target triple
    Given the binary was built from commit "abc1234" on "2026-03-24"
    When I run "aegis --version"
    Then the output should match the pattern "0.1.0 (abc1234 2026-03-24 x86_64-unknown-linux-musl)"
    And the SHA should be exactly 7 characters

  # @req REQ-BUILD-012
  Scenario: aegis --version includes build date from SOURCE_DATE_EPOCH
    Given SOURCE_DATE_EPOCH was set to "1711324800" during build
    When I run "aegis --version"
    Then the build date in the version string should be "2024-03-25"

  # @req REQ-BUILD-012
  Scenario: Version string is absent when built outside git repository
    Given the source is extracted from a tarball without .git directory
    When I build and run "aegis --version"
    Then the SHA field should display "unknown" instead of a commit hash

  # ---------------------------------------------------------------------------
  # REQ-BUILD-013: sccache and cargo registry cache for sub-5-min CI builds
  # ---------------------------------------------------------------------------

  # @req REQ-BUILD-013
  Scenario: Incremental CI build completes in under 5 minutes on warm cache
    Given the CI cache key is derived from Cargo.lock and rust-toolchain.toml
    And the cache has been populated by a previous build
    When the CI pipeline runs an incremental build with no source changes
    Then the build should complete in under 300 seconds
    And sccache should report a cache hit ratio above 90%

  # @req REQ-BUILD-013
  Scenario: Cache key changes when Cargo.lock is updated
    Given a prior CI cache exists for the old Cargo.lock hash
    When a dependency is updated and Cargo.lock changes
    Then the CI cache key should differ from the previous run
    And the build should proceed with a cold cache for changed crates

  # @req REQ-BUILD-013
  Scenario: Untrusted PR builds use read-only cache
    Given a PR from a fork triggers the CI pipeline
    When the build accesses the sccache layer
    Then the cache should be read-only
    And no cache writes should occur from the untrusted PR build

  # ---------------------------------------------------------------------------
  # REQ-BUILD-014: VHS demo tape scripts
  # ---------------------------------------------------------------------------

  # @req REQ-BUILD-014
  Scenario: All VHS tape scripts parse without errors
    Given VHS is installed
    When each tape file in docs/demos/tapes/ is validated
    Then vhs validate should exit 0 for every tape

  # @req REQ-BUILD-015
  Scenario: CI generates GIFs from tape scripts on release tag
    Given a release tag is pushed
    When the demo-gifs CI job runs
    Then each tape should produce a GIF in docs/demos/gifs/
    And all GIFs should be under 5MB
    And GIFs should be attached to the GitHub release

  # @req REQ-BUILD-016
  Scenario: README embeds demo GIFs with valid links
    Given the README.md file
    When all image links are extracted
    Then each GIF link should point to an existing file in docs/demos/gifs/

  # ---------------------------------------------------------------------------
  # REQ-BUILD-039: CycloneDX SBOM generation
  # ---------------------------------------------------------------------------

  # @req REQ-BUILD-039
  Scenario: CI generates CycloneDX SBOM on every push
    Given the aegis-cli workspace is checked out
    When the sbom CI job runs cargo cyclonedx --format json --spec-version 1.5
    Then a file bom.json should exist at the workspace root
    And the file should parse as valid JSON
    And the file should contain bomFormat field equal to CycloneDX

  # @req REQ-BUILD-039
  Scenario: SBOM is uploaded as a CI artifact
    Given the sbom CI job has completed successfully
    When artifacts are listed for the workflow run
    Then an artifact named aegis-cli-sbom should be present
    And the artifact should contain bom.json

  # @req REQ-BUILD-039
  Scenario: SBOM generation requires no secrets
    Given a fork of aegis-cli opens a pull request
    When the sbom CI job runs
    Then it should complete successfully without access to repository secrets

  # ---------------------------------------------------------------------------
  # REQ-BUILD-040: GPG signing of Linux release artifacts (future)
  # REQ-BUILD-041: Authenticode signing of Windows release artifacts (future)
  # Skipped until organizational signing infrastructure is available.
  # ---------------------------------------------------------------------------

  # ---------------------------------------------------------------------------
  # REQ-BUILD-042: cargo-deb unsigned package generation
  # ---------------------------------------------------------------------------

  # @req REQ-BUILD-042
  Scenario: cargo-deb produces a valid .deb package
    Given Cargo.toml contains [package.metadata.deb] section for aegis-cli
    When CI runs cargo deb --package aegis-cli
    Then a file target/debian/aegis-cli_*.deb should exist
    And the file should be a valid Debian package per dpkg-deb -I

  # ---------------------------------------------------------------------------
  # REQ-BUILD-043: cargo-generate-rpm unsigned package generation
  # ---------------------------------------------------------------------------

  # @req REQ-BUILD-043
  Scenario: cargo-generate-rpm produces a valid .rpm package
    Given Cargo.toml contains [package.metadata.generate-rpm] section for aegis-cli
    When CI runs cargo generate-rpm --package aegis-cli
    Then a file target/generate-rpm/aegis-cli-*.rpm should exist
    And the file should be a valid RPM per rpm -qip

  # ---------------------------------------------------------------------------
  # REQ-BUILD-045: deb install smoke test
  # ---------------------------------------------------------------------------

  # @req REQ-BUILD-045
  Scenario: .deb package installs and runs on Ubuntu
    Given an Ubuntu CI runner with the generated .deb file
    When dpkg -i target/debian/aegis-cli_*.deb is executed
    And aegis --version is run
    Then the install should succeed with exit code 0
    And aegis --version should print the expected version string

  # ---------------------------------------------------------------------------
  # REQ-BUILD-046: rpm install smoke test
  # ---------------------------------------------------------------------------

  # @req REQ-BUILD-046
  Scenario: .rpm package installs and runs on RHEL 9
    Given a RHEL 9 container with the generated .rpm file
    When rpm -i target/generate-rpm/aegis-cli-*.rpm is executed
    And aegis --version is run
    Then the install should succeed with exit code 0
    And aegis --version should print the expected version string
