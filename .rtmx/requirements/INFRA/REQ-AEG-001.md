# REQ-AEG-001: Infrastructure Automation via `aegis init`

## Overview
The `aegis-cli` must provide a deterministic way to provision its own secure cloud backend to eliminate configuration drift and manual errors.

## Specification
- Utilize `@pulumi/pulumi/automation` to embed Pulumi execution within the Node.js process.
- Support "Self-Service BYOC" mode where the CLI provisions a hardened GCP Assured Workloads boundary.
- Automate `preview`, `up`, and `destroy` lifecycles programmatically.
- Captures outputs (VPC name, KMS Key IDs, Endpoint URLs) and writes them to `~/.aegis/config.yaml`.

## Acceptance Criteria
- `aegis init` completes successfully without external Pulumi CLI installation.
- Infrastructure outputs are correctly persisted to local configuration.
- The provisioned resources match the compliance blueprint defined in Section 4 of `GEMINI.md`.

## BDD Scenarios

### Scenario 1: Successful Self-Service Initialization
**Given** the user has valid Google Cloud Application Default Credentials (ADC)
**When** the user executes `aegis init` and selects "Self-Service BYOC"
**Then** the Pulumi Automation API should trigger a stack update
**And** the VPC, KMS Key, and Vertex AI endpoint should be provisioned
**And** the configuration should be saved to `~/.aegis/config.yaml` with `600` permissions.

### Scenario 2: Initialization Fails Due to Missing Credentials
**Given** no Google Cloud credentials are detected on the workstation
**When** the user executes `aegis init`
**Then** the "Environment Probe" (State 0) should identify the missing credentials
**And** the system should display an error message directing the user to run `gcloud auth application-default login`
**And** the state machine should not proceed to "Infrastructure Binding".

### Scenario 3: Recovery from Interrupted Pulumi Update
**Given** a previous `aegis init` was interrupted during the "Infrastructure Binding" phase
**When** the user re-executes `aegis init`
**Then** the Pulumi Automation API should detect the existing stack state
**And** it should perform a refresh to synchronize the local state with GCP
**And** it should resume the update to reach the desired configuration.

### Scenario 4: Tearing Down Infrastructure (`destroy`)
**Given** an existing Aegis infrastructure provisioned via "Self-Service" mode
**When** the user executes `aegis destroy`
**Then** the Pulumi Automation API should trigger a stack destruction
**And** all GCP resources (VPC, KMS, Vertex AI endpoint) should be removed
**And** the local `~/.aegis/config.yaml` should be updated to reflect the removal of the backend.

### Scenario 5: Infrastructure Compliance Check (Dry-run/Preview)
**Given** a modification to the embedded Pulumi TypeScript program
**When** the user executes `aegis init --preview`
**Then** the Pulumi Automation API should generate a preview of resource changes
**And** it should verify that the proposed resource graph remains compliant with the NIST 800-171 controls defined in the blueprint.

## TDD Test Case Signatures

| Test Case ID | Signature | Expected Result |
| :--- | :--- | :--- |
| **TDD-AEG-001-01** | `TestPulumiAutomation_Init_Success(t *testing.T)` | Mocks Pulumi `up()` and asserts that `config.yaml` contains correct resource IDs. |
| **TDD-AEG-001-02** | `TestPulumiAutomation_MissingCredentials(t *testing.T)` | Mocks a failure in `gcloud` ADC detection and asserts that an `ErrMissingCredentials` is returned. |
| **TDD-AEG-001-03** | `TestConfig_FilePermissions(t *testing.T)` | Asserts that `os.Stat()` on the generated config file returns `0600` (read/write only by owner). |
| **TDD-AEG-001-04** | `TestPulumi_StackOutputs_Mapping(t *testing.T)` | Verifies that Pulumi stack outputs (e.g., `kmsKeyId`) are correctly mapped to the internal `Config` struct. |
| **TDD-AEG-001-05** | `TestPulumi_Destroy_Success(t *testing.T)` | Mocks Pulumi `destroy()` and verifies that all resources are marked for deletion in the stack state. |
| **TDD-AEG-001-06** | `TestPulumi_Preview_ComplianceCheck(t *testing.T)` | Asserts that a preview command returns a valid plan without executing any cloud mutations. |
| **TDD-AEG-001-07** | `TestPulumi_Automated_Retry_On_Transient_Error(t *testing.T)` | Verifies that the Automation API client retries on transient GCP 503 errors during `up()`. |

## Traceability
- **Parent:** GEMINI.md Section 4
- **Tests:** `pkg/infra/pulumi_test.go`
