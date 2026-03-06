# REQ-AEG-008: Secure Onboarding State Machine

## Overview
Aegis must implement a reactive state machine to bootstrap trust on a local workstation.

## Specification
- **State 0: Environment Probe:** Scans for existing GCP ADC and corporate proxy settings.
- **State 1: Mode Selection:** Prompts for Self-Service, Enterprise, or Managed SaaS.
- **State 2: Credential Negotiation:** Auth via ADC, SSO callback, or JWT/PKCE.
- **State 3: Infrastructure Binding:** Connects to backend and retrieves endpoints.
- **State 4: Configuration Commit:** Writes `~/.aegis/config.yaml` with strict `600` permissions.

## Acceptance Criteria
- Configuration file contains NO secrets, tokens, or CUI.
- Initialization handles different network topologies (e.g., proxies or air-gapped corporate gateways).
- Mode swapping works without requiring a complete re-initialization.

## BDD Scenarios

### Scenario 1: Initial Onboarding (State 0 to State 4)
**Given** a first-time user executes `aegis init`
**When** the "Environment Probe" (State 0) detects valid GCP credentials
**And** the user selects "Self-Service BYOC" in "Mode Selection" (State 1)
**And** the "Infrastructure Binding" (State 3) successfully connects to GCP
**Then** the "Configuration Commit" (State 4) should write `~/.aegis/config.yaml`
**And** the onboarding should complete successfully.

### Scenario 2: Selecting Enterprise Mode with SSO
**Given** the user is an enterprise employee
**When** the user selects "Enterprise" in "Mode Selection" (State 1)
**Then** the "Credential Negotiation" (State 2) should trigger an SSO callback
**And** once authenticated, it should transition to "Infrastructure Binding" (State 3)
**And** successfully retrieve the corporate Aegis endpoint.

### Scenario 3: Transition from Mode Selection to Credential Negotiation
**Given** the user is in "Mode Selection" (State 1)
**When** the user selects "Managed SaaS"
**Then** the state machine must transition to "Credential Negotiation" (State 2)
**And** it should initiate an OAuth 2.0 PKCE flow in the system browser.

### Scenario 4: Environment Probe with Proxy Detection (State 0)
**Given** a corporate network environment requiring a TLS-inspecting proxy
**When** the user executes `aegis init`
**Then** the "Environment Probe" (State 0) should detect `HTTPS_PROXY` and `NODE_EXTRA_CA_CERTS` environment variables
**And** it should configure the global Node.js `https.Agent` to utilize these settings for all cloud communication.

### Scenario 5: Mode Swapping without Full Re-initialization
**Given** an existing and valid "Self-Service" configuration in `~/.aegis/config.yaml`
**When** the user executes a mode swap command (e.g., `aegis config set mode enterprise`)
**Then** the state machine should re-enter "Mode Selection" (State 1)
**And** it should preserve the results of the initial "Environment Probe" (State 0) to avoid redundant workstation scans.

### Scenario 6: Configuration Commit (State 4) contains NO secrets
**Given** a successful completion of the onboarding state machine
**When** the "Configuration Commit" (State 4) writes the `~/.aegis/config.yaml` file
**Then** the file must only contain routing metadata (Project IDs, Region, Endpoints)
**And** it must not contain any session tokens, private keys, or CUI.

## TDD Test Case Signatures

| Test Case ID | Signature | Expected Result |
| :--- | :--- | :--- |
| **TDD-AEG-008-01** | `TestStateMachine_Initial_Transition_0_1(t *testing.T)` | Asserts that a successful probe transitions to State 1. |
| **TDD-AEG-008-02** | `TestStateMachine_ModeSelection_BYOC(t *testing.T)` | Verifies that selecting BYOC sets the internal `Mode` to `SelfService`. |
| **TDD-AEG-008-03** | `TestStateMachine_CredentialNegotiation_Fail(t *testing.T)` | Asserts that a failed SSO auth returns the user to State 1. |
| **TDD-AEG-008-04** | `TestStateMachine_ConfigCommit_Permissions(t *testing.T)` | Asserts that State 4 results in a config file with 600 permissions. |
| **TDD-AEG-008-05** | `TestStateMachine_ProxyDetection_State0(t *testing.T)` | Mocks proxy environment variables and asserts that they are correctly loaded into the Probe result. |
| **TDD-AEG-008-06** | `TestStateMachine_ModeSwapping_PreservesProbe(t *testing.T)` | Verifies that the internal state machine can jump between states without resetting the Probe data. |
| **TDD-AEG-008-07** | `TestStateMachine_Config_NoSecretsStored(t *testing.T)` | Scans the generated `Config` struct for sensitive keywords and asserts they are null/empty. |


## Traceability
- **Parent:** GEMINI.md Section 2
- **Tests:** `pkg/onboard/machine_test.go`
