@wip
Feature: OS Keychain Credential Storage
  As a defense engineer storing OAuth refresh tokens
  I need tokens stored in the OS-native credential manager
  So that secrets never appear in config files, environment variables, or logs

  Background:
    Given the aegis-security crate is initialized

  # ---------------------------------------------------------------------------
  # REQ-SECURITY-019: OS keychain storage for refresh tokens
  # ---------------------------------------------------------------------------

  # @req REQ-SECURITY-019
  Scenario: Store and retrieve a refresh token via the keychain provider
    Given the keychain backend is configured as "auto"
    When a refresh token "rt_abc123" is stored for service "aegis-cli" account "refresh-token-vertex"
    Then retrieve("aegis-cli", "refresh-token-vertex") should return "rt_abc123"

  # @req REQ-SECURITY-019
  Scenario: Delete a refresh token from the keychain
    Given a refresh token exists in the keychain for "refresh-token-vertex"
    When delete("aegis-cli", "refresh-token-vertex") is called
    Then retrieve("aegis-cli", "refresh-token-vertex") should return None

  # @req REQ-SECURITY-019
  Scenario: Retrieve returns None when no credential exists
    Given no credential exists for "refresh-token-bedrock"
    When retrieve("aegis-cli", "refresh-token-bedrock") is called
    Then it should return None without error

  # @req REQ-SECURITY-019
  Scenario: Token rotation overwrites existing keychain entry
    Given a refresh token "rt_old" exists in the keychain for "refresh-token-vertex"
    When store("aegis-cli", "refresh-token-vertex", "rt_new") is called
    Then retrieve("aegis-cli", "refresh-token-vertex") should return "rt_new"
    And the old token "rt_old" should no longer be accessible

  # @req REQ-SECURITY-019
  Scenario: macOS backend uses Security.framework via security-framework crate
    Given the platform is macOS
    And keychain_backend is "auto" or "macos"
    When a token is stored
    Then it should be stored in the macOS Keychain via SecItemAdd
    And the entry should be accessible only to the current user

  # @req REQ-SECURITY-019
  Scenario: Windows backend uses Windows Credential Manager
    Given the platform is Windows
    And keychain_backend is "auto" or "windows"
    When a token is stored
    Then it should be stored via CredWriteW in Windows Credential Manager
    And the entry should be protected by the user's Windows DPAPI key

  # @req REQ-SECURITY-019
  Scenario: Linux backend uses libsecret when available
    Given the platform is Linux
    And the Secret Service D-Bus API is available (GNOME Keyring or KDE Wallet)
    And keychain_backend is "auto" or "libsecret"
    When a token is stored
    Then it should be stored via the Secret Service D-Bus protocol

  # @req REQ-SECURITY-019
  Scenario: Linux fallback uses encrypted file when libsecret unavailable
    Given the platform is Linux
    And no Secret Service D-Bus API is available (headless server or air-gapped)
    And keychain_backend is "auto" or "file"
    When a token is stored
    Then it should be stored in ~/.aegis/credentials.enc
    And the file should be encrypted with a key derived via Argon2
    And the file permissions should be 0600

  # @req REQ-SECURITY-019
  Scenario: Refresh tokens never appear in config.yaml
    Given a refresh token is stored via the keychain
    When aegis writes config.yaml
    Then config.yaml should not contain any refresh_token or access_token fields
    And only the keychain_backend setting should appear in the config

  # @req REQ-SECURITY-019
  Scenario: aegis logout clears all keychain entries
    Given refresh tokens exist for "vertex" and "bedrock" in the keychain
    When the user runs "aegis logout" or credential rotation
    Then all aegis-cli keychain entries should be deleted
    And the AuthManager credential cache should be cleared
