# REQ-AEG-007: Assured Workloads Boundary (GCP)

## Overview
Aegis must operate within a provably secure backend boundary that satisfies NIST SP 800-171 and CMMC Level 2 data residency requirements.

## Specification
- **GCP Assured Workloads:** Hardened US regions only (`us-central1`).
- **Cloud KMS (CMEK):** All data at rest must be encrypted via FIPS-validated KMS keys.
- **VPC Service Controls (VPC-SC):** API firewall around `aiplatform.googleapis.com` to prevent unauthorized model access.
- **Audit Logging:** Enable `ADMIN_READ`, `DATA_READ`, and `DATA_WRITE` audit logs globally.

## Acceptance Criteria
- Cloud Audit Logs capture all Vertex AI interaction metadata.
- KMS Keys rotate automatically every 30 days.
- VPC-SC rejects requests from unauthorized IP address space or managed devices.

## BDD Scenarios

### Scenario 1: Accessing Vertex AI from Authorized IP within VPC-SC Perimeter
**Given** the user is connected to the secure VPC
**When** the user sends a request to the Vertex AI endpoint
**Then** the request should be authorized by the Service Perimeter
**And** the AI agent should successfully return a response
**And** a `DATA_READ` event should be logged in the GCP Cloud Audit Logs.

### Scenario 2: Accessing Vertex AI from Unauthorized IP (Blocked by VPC-SC)
**Given** the user is on an unauthorized public network
**When** the user attempts to interact with the Aegis Vertex AI endpoint
**Then** VPC-SC should intercept the request
**And** the system must return a `403 Permission Denied` error
**And** the access attempt should be recorded in the GCP VPC-SC access logs.

### Scenario 3: Verification of CMEK Encryption for Stored Audit Logs
**Given** the GCP audit logs are stored in a Cloud Storage bucket
**When** the security administrator checks the bucket's encryption settings
**Then** it should confirm that the bucket is encrypted using the Aegis-provisioned KMS key
**And** the key should show a rotation policy of exactly 30 days.

### Scenario 4: Audit Log Retention Policy Enforcement
**Given** the GCP audit logs are stored in a Cloud Storage bucket
**When** the lifecycle policy is evaluated
**Then** it should ensure that logs are retained for exactly 365 days
**And** logs older than 365 days are automatically deleted to balance compliance with data minimization.

### Scenario 5: FIPS 140-2 Validated TLS 1.3 Transit
**Given** the Aegis CLI is communicating with the Vertex AI endpoint
**When** the network connection is established
**Then** the client must enforce TLS 1.3 with FIPS-validated cipher suites
**And** any attempt to downgrade to an insecure protocol (e.g., TLS 1.2 or SSLv3) must be rejected by the CLI.

## TDD Test Case Signatures

| Test Case ID | Signature | Expected Result |
| :--- | :--- | :--- |
| **TDD-AEG-007-01** | `TestVPCSC_Enforcement_Mock(t *testing.T)` | Asserts that a request with an unauthorized source IP triggers a 403 response. |
| **TDD-AEG-007-02** | `TestKMS_RotationPolicy_Verification(t *testing.T)` | Verifies that the Pulumi code specifies a 30-day rotation period for the KMS key. |
| **TDD-AEG-007-03** | `TestAuditLog_DataRead_Logged(t *testing.T)` | Asserts that a Vertex AI request generates a corresponding entry in the mock audit log. |
| **TDD-AEG-007-04** | `TestBoundary_RegionalLock_Validation(t *testing.T)` | Asserts that any attempt to use a non-US-central1 region is rejected by the infrastructure engine. |
| **TDD-AEG-007-05** | `TestBucket_LifecyclePolicy_Retention365Days(t *testing.T)` | Verifies that the GCS bucket lifecycle rule specifies a `Delete` action with `age: 365`. |
| **TDD-AEG-007-06** | `TestTLS_CipherSuite_FIPS_Compliance(t *testing.T)` | Asserts that the Node.js `https` client is configured with FIPS-approved ciphers. |
| **TDD-AEG-007-07** | `TestVPC_PrivateIpGoogleAccess_Enabled(t *testing.T)` | Verifies that the provisioned subnet has `privateIpGoogleAccess` set to `true`. |


## Traceability
- **Parent:** GEMINI.md Section 4
- **Tests:** `pkg/infra/boundary_test.go`
