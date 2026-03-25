Feature: GCP Assured Workloads Provisioning
  As a defense engineer using Self-Service BYOC mode
  I need aegis init to provision a compliant GCP boundary
  So that my Vertex AI interactions satisfy NIST 800-171 and IL4/IL5

  # ---------------------------------------------------------------------------
  # REQ-INFRA-001: Embedded Pulumi IaC for GCP Assured Workloads (IL4/IL5)
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-001
  Scenario: Successful GCP Assured Workloads provisioning
    Given valid GCP Application Default Credentials
    And the GCP project has Assured Workloads API enabled
    When "aegis init" provisions the GCP boundary
    Then an Assured Workloads folder should be created
    And a Cloud KMS CMEK key should be created with 30-day rotation
    And a VPC with Private Google Access should be created
    And VPC Service Controls should restrict aiplatform.googleapis.com and storage.googleapis.com
    And Cloud Audit Logs should be enabled for DATA_READ, DATA_WRITE, and ADMIN for aiplatform
    And audit logs should be routed to a CMEK-encrypted Storage bucket
    And the Vertex AI endpoint should be pinned to a specific model version
    And all resources should be in US regions only

  # @req REQ-INFRA-001
  Scenario: Provisioning fails with missing GCP credentials
    Given no GCP credentials on the workstation
    When the user executes "aegis init"
    Then the environment probe should detect missing credentials
    And display an error directing the user to run "gcloud auth application-default login"
    And no infrastructure should be provisioned
    And the exit code should be non-zero

  # @req REQ-INFRA-001
  Scenario: Provisioning fails when project lacks Assured Workloads API
    Given valid GCP credentials
    And the GCP project does not have Assured Workloads API enabled
    When "aegis init" attempts provisioning
    Then aegis should display "Assured Workloads API not enabled on project"
    And provide a remediation command to enable the API
    And no resources should be created

  # ---------------------------------------------------------------------------
  # REQ-INFRA-004: Infrastructure preview (dry-run) before provisioning
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-004
  Scenario: aegis plan outputs resource diff with no side effects
    Given valid GCP credentials
    When the user executes "aegis plan"
    Then the output should list all resources that would be created
    And include Assured Workloads folder, KMS key, VPC, VPC-SC perimeter, audit sink
    And no resources should be created or modified in GCP

  # @req REQ-INFRA-004
  Scenario: aegis plan detects drift from desired state
    Given an existing GCP boundary provisioned by aegis
    And the VPC-SC perimeter has been manually modified
    When the user executes "aegis plan"
    Then the output should show the VPC-SC perimeter as "update" with a diff
    And no changes should be applied

  # ---------------------------------------------------------------------------
  # REQ-INFRA-005: Infrastructure drift detection
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-005
  Scenario: aegis drift reports configuration drift within 60 seconds
    Given an existing GCP boundary provisioned by aegis
    And VPC-SC perimeter restricted_services has been manually altered
    When the user executes "aegis drift"
    Then the report should complete within 60 seconds
    And flag the VPC-SC perimeter drift with severity "HIGH"

  # @req REQ-INFRA-005
  Scenario: aegis drift reports no drift when infrastructure matches state
    Given an existing GCP boundary with no manual modifications
    When the user executes "aegis drift"
    Then the report should indicate "No drift detected"
    And exit with code 0

  # ---------------------------------------------------------------------------
  # REQ-INFRA-006: Pulumi state encryption at rest with CMEK
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-006
  Scenario: Pulumi state file is encrypted with Cloud KMS CMEK
    Given a GCP boundary has been provisioned
    When I inspect the Pulumi state backend in GCS
    Then the state file should be encrypted with the project CMEK key
    And the state file should never exist in plaintext on disk or in GCS

  # @req REQ-INFRA-006
  Scenario: State decryption fails when KMS key is disabled
    Given the CMEK key used for state encryption has been disabled
    When the user executes "aegis plan"
    Then aegis should fail with "unable to decrypt Pulumi state: KMS key is disabled"
    And no infrastructure operations should proceed

  # ---------------------------------------------------------------------------
  # REQ-INFRA-007: Infrastructure state backup to secondary GCS bucket
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-007
  Scenario: State is replicated to secondary US region
    Given a GCP boundary with primary state in "us-central1"
    When a Pulumi state update occurs
    Then the state should be replicated to a secondary bucket in "us-east4"
    And both copies should be CMEK-encrypted

  # @req REQ-INFRA-007
  Scenario: State backup detects replication lag
    Given the secondary state bucket exists
    When the primary state is updated
    And the secondary copy is more than 5 minutes behind
    Then aegis should log a warning "State replication lag exceeds threshold"

  # ---------------------------------------------------------------------------
  # REQ-INFRA-008: Multi-region with data-residency enforcement (US only)
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-008
  Scenario: Non-US region is rejected before provisioning
    Given the user attempts "aegis init" with region "europe-west1"
    When the region validation runs
    Then aegis should reject the region with "Non-US region not permitted for IL4/IL5 workloads"
    And no provisioning should occur

  # @req REQ-INFRA-008
  Scenario: US region is accepted for provisioning
    Given the user attempts "aegis init" with region "us-central1"
    When the region validation runs
    Then the region should be accepted
    And provisioning should proceed

  # ---------------------------------------------------------------------------
  # REQ-INFRA-009: KMS key rotation verification post-provisioning
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-009
  Scenario: aegis verify --check kms-rotation confirms 30-day schedule
    Given a provisioned GCP boundary with a CMEK key
    When the user runs "aegis verify --check kms-rotation"
    Then the output should confirm rotation period is 30 days or less
    And at least one key version should be ENABLED

  # @req REQ-INFRA-009
  Scenario: aegis verify --check kms-rotation fails when rotation exceeds 30 days
    Given a provisioned GCP boundary
    And the KMS key rotation period has been manually changed to 90 days
    When the user runs "aegis verify --check kms-rotation"
    Then the check should FAIL with "rotation period 90d exceeds maximum 30d"

  # ---------------------------------------------------------------------------
  # REQ-INFRA-010: VPC-SC perimeter validation
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-010
  Scenario: aegis verify --check vpc-sc confirms active perimeter
    Given a provisioned GCP boundary
    When the user runs "aegis verify --check vpc-sc"
    Then the output should confirm perimeter status is "ACTIVE"
    And "aiplatform.googleapis.com" should be in restricted_services
    And "storage.googleapis.com" should be in restricted_services

  # @req REQ-INFRA-010
  Scenario: aegis verify --check vpc-sc detects inactive perimeter
    Given a provisioned GCP boundary
    And the VPC-SC perimeter has been manually set to DRY_RUN mode
    When the user runs "aegis verify --check vpc-sc"
    Then the check should FAIL with "VPC-SC perimeter is DRY_RUN, expected ACTIVE"

  # ---------------------------------------------------------------------------
  # REQ-INFRA-011: Audit log sink verification post-provisioning
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-011
  Scenario: aegis verify --check audit-sink confirms active sink
    Given a provisioned GCP boundary
    When the user runs "aegis verify --check audit-sink"
    Then the output should confirm the audit log sink is active
    And DATA_READ, DATA_WRITE, and ADMIN_READ log types should be enabled for aiplatform
    And the destination bucket should be CMEK-encrypted

  # @req REQ-INFRA-011
  Scenario: aegis verify --check audit-sink fails when sink is disabled
    Given a provisioned GCP boundary
    And the audit log sink has been manually disabled
    When the user runs "aegis verify --check audit-sink"
    Then the check should FAIL with "audit log sink is disabled"

  # ---------------------------------------------------------------------------
  # REQ-INFRA-012: Endpoint connectivity verification
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-012
  Scenario: aegis verify --check endpoint confirms Vertex AI reachable
    Given a provisioned GCP boundary with Vertex AI endpoint configured
    When the user runs "aegis verify --check endpoint"
    Then the check should confirm TLS connection succeeds
    And the certificate chain should be valid
    And the model version should match the pinned version in config

  # @req REQ-INFRA-012
  Scenario: aegis verify --check endpoint fails when endpoint is unreachable
    Given a provisioned GCP boundary
    And the Vertex AI endpoint is blocked by a firewall rule
    When the user runs "aegis verify --check endpoint"
    Then the check should FAIL with "endpoint unreachable"
    And the error should include the endpoint URL and timeout duration

  # ---------------------------------------------------------------------------
  # REQ-INFRA-013: Teardown safety gate with mandatory typed confirmation
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-013
  Scenario: aegis destroy requires typed project name for confirmation
    Given a provisioned GCP boundary in project "aegis-prod-001"
    When the user executes "aegis destroy"
    Then aegis should display the complete resource tree to be destroyed
    And prompt "Type the project name 'aegis-prod-001' to confirm destruction"
    And the teardown should not proceed until the exact project name is typed

  # @req REQ-INFRA-013
  Scenario: aegis destroy aborts on incorrect confirmation
    Given a provisioned GCP boundary in project "aegis-prod-001"
    When the user executes "aegis destroy" and types "aegis-prod-002"
    Then aegis should display "Confirmation does not match. Teardown aborted."
    And no resources should be destroyed

  # @req REQ-INFRA-013
  Scenario: aegis destroy logs the teardown decision to audit ledger
    Given a provisioned GCP boundary
    When the user confirms destruction
    Then the audit ledger should record "INFRA_DESTROY_CONFIRMED" with the project name
    And after teardown completes, "INFRA_DESTROY_COMPLETED" should be logged

  # ---------------------------------------------------------------------------
  # REQ-INFRA-014: NIST 800-171 compliance validation
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-014
  Scenario: aegis verify --compliance maps resources to NIST controls
    Given a fully provisioned GCP boundary
    When the user runs "aegis verify --compliance"
    Then the output should map each provisioned resource to NIST 800-171 control IDs
    And each control should show PASS, FAIL, or WARN status
    And the overall compliance status should be displayed

  # @req REQ-INFRA-014
  Scenario: Compliance check fails when required resource is missing
    Given a GCP boundary missing the VPC-SC perimeter
    When the user runs "aegis verify --compliance"
    Then control 3.13.1 (boundary protection) should show FAIL
    And the overall status should be "NON-COMPLIANT"

  # ---------------------------------------------------------------------------
  # REQ-INFRA-015: Infrastructure health monitoring
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-015
  Scenario: aegis status --watch polls infrastructure health every 5 minutes
    Given a provisioned GCP boundary
    When the user runs "aegis status --watch"
    Then aegis should query Cloud Asset Inventory every 5 minutes
    And display the health of KMS, VPC-SC, audit sink, and endpoint
    And alerts should be written to the audit ledger

  # @req REQ-INFRA-015
  Scenario: Infrastructure alert surfaces when VPC-SC perimeter is degraded
    Given "aegis status --watch" is running
    And the VPC-SC perimeter transitions to DRY_RUN externally
    When the next health poll executes
    Then the TUI should display an alert "VPC-SC perimeter degraded: DRY_RUN"
    And the audit ledger should record "INFRA_HEALTH_ALERT"

  # ---------------------------------------------------------------------------
  # REQ-INFRA-016: Cost estimation before provisioning
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-016
  Scenario: aegis plan --estimate shows monthly cost with Assured Workloads premium
    Given valid GCP credentials
    When the user runs "aegis plan --estimate"
    Then the output should include estimated monthly cost for each resource
    And the Assured Workloads 20% premium should be itemized
    And the user should be prompted to acknowledge the cost before proceeding

  # @req REQ-INFRA-016
  Scenario: Cost estimate updates when resource configuration changes
    Given a previous cost estimate of $500/month
    When the user modifies the region or model version in config
    And runs "aegis plan --estimate"
    Then the new estimate should reflect the configuration change

  # ---------------------------------------------------------------------------
  # REQ-INFRA-017: Mandatory compliance metadata labels on all resources
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-017
  Scenario: All GCP resources carry required compliance labels
    Given a provisioned GCP boundary
    When I inspect the labels on all provisioned resources
    Then each resource should have labels: environment, classification, owner, cost-center
    And no resource should be missing any mandatory label

  # @req REQ-INFRA-017
  Scenario: Pulumi fails if mandatory labels are absent from resource definition
    Given the Pulumi program omits the "classification" label from a resource
    When "aegis init" attempts provisioning
    Then the Pulumi preview should fail with "missing mandatory label: classification"
    And no resources should be created
