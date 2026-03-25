Feature: Multi-Cloud Infrastructure Provisioning
  As a defense engineer deploying aegis across AWS GovCloud and Azure Government
  I need aegis to provision compliant infrastructure boundaries on multiple clouds
  So that I can use Bedrock and Azure OpenAI while meeting NIST 800-171 requirements

  # ---------------------------------------------------------------------------
  # REQ-INFRA-002: Embedded Pulumi IaC for AWS GovCloud
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-002
  Scenario: Successful AWS GovCloud provisioning
    Given valid AWS credentials for a GovCloud account
    And the region is set to "us-gov-west-1"
    When "aegis init --cloud aws" provisions the AWS GovCloud boundary
    Then an AWS KMS key should be created with automatic rotation
    And a VPC with PrivateLink to Bedrock should be created
    And CloudTrail logging should be enabled
    And an S3 bucket for audit logs should be created with KMS encryption
    And the Bedrock endpoint should be pinned

  # @req REQ-INFRA-002
  Scenario: AWS provisioning fails with missing GovCloud credentials
    Given no AWS credentials are available
    When the user executes "aegis init --cloud aws"
    Then the environment probe should detect missing credentials
    And display an error directing the user to configure AWS credentials
    And no infrastructure should be provisioned

  # ---------------------------------------------------------------------------
  # REQ-INFRA-003: Embedded Pulumi IaC for Azure Government
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-003
  Scenario: Successful Azure Government provisioning
    Given valid Azure Government credentials
    And the region is set to "usgovvirginia"
    When "aegis init --cloud azure" provisions the Azure Gov boundary
    Then an Azure Key Vault should be created with key rotation
    And a VNet with Private Endpoint to Azure OpenAI should be created
    And Azure Monitor diagnostic settings should be enabled
    And a Storage Account for audit logs should be created with Key Vault encryption
    And the Azure OpenAI endpoint should be pinned

  # @req REQ-INFRA-003
  Scenario: Azure provisioning fails with missing Azure Gov credentials
    Given no Azure credentials are available
    When the user executes "aegis init --cloud azure"
    Then the environment probe should detect missing credentials
    And display an error directing the user to run "az login --cloud AzureUSGovernment"
    And no infrastructure should be provisioned

  # ---------------------------------------------------------------------------
  # REQ-INFRA-018: AWS GovCloud infrastructure preview (dry-run)
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-018
  Scenario: aegis plan --cloud aws outputs resource diff without side effects
    Given valid AWS GovCloud credentials
    When the user executes "aegis plan --cloud aws"
    Then the output should list all resources that would be created in AWS GovCloud
    And include KMS key, VPC, PrivateLink, CloudTrail, S3 bucket
    And no resources should be created or modified in AWS

  # @req REQ-INFRA-018
  Scenario: aegis plan --cloud aws detects drift from desired state
    Given an existing AWS GovCloud boundary
    And a security group rule has been manually added
    When the user executes "aegis plan --cloud aws"
    Then the output should show the security group as "update" with a diff

  # ---------------------------------------------------------------------------
  # REQ-INFRA-019: AWS GovCloud drift detection
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-019
  Scenario: aegis drift --cloud aws reports resource drift
    Given an existing AWS GovCloud boundary
    And the CloudTrail logging has been manually disabled
    When the user executes "aegis drift --cloud aws"
    Then the report should flag CloudTrail drift with severity "CRITICAL"
    And classify it under the "audit" category

  # @req REQ-INFRA-019
  Scenario: aegis drift --cloud aws reports no drift when compliant
    Given an existing AWS GovCloud boundary with no manual modifications
    When the user executes "aegis drift --cloud aws"
    Then the report should indicate "No drift detected"

  # ---------------------------------------------------------------------------
  # REQ-INFRA-020: AWS GovCloud teardown safety gate
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-020
  Scenario: aegis destroy --cloud aws requires typed account ID confirmation
    Given a provisioned AWS GovCloud boundary in account "123456789012"
    When the user executes "aegis destroy --cloud aws"
    Then aegis should display the complete resource tree
    And prompt the user to type the account ID to confirm
    And not proceed until the exact ID is typed

  # @req REQ-INFRA-020
  Scenario: aegis destroy --cloud aws is blocked if CloudTrail is disabled
    Given a provisioned AWS GovCloud boundary
    And CloudTrail has been manually disabled
    When the user executes "aegis destroy --cloud aws"
    Then aegis should refuse teardown with "Cannot destroy: CloudTrail is disabled"
    And require the user to re-enable CloudTrail before proceeding

  # ---------------------------------------------------------------------------
  # REQ-INFRA-021: AWS GovCloud mandatory compliance tags
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-021
  Scenario: All AWS resources carry required compliance tags
    Given a provisioned AWS GovCloud boundary
    When I inspect the tags on all provisioned resources
    Then each resource should have tags: Environment, Classification, Owner, CostCenter
    And the tag schema should match the GCP label schema

  # @req REQ-INFRA-021
  Scenario: Pulumi fails if mandatory tags are absent from AWS resource
    Given the Pulumi program omits the "Classification" tag from an S3 bucket
    When "aegis init --cloud aws" attempts provisioning
    Then the Pulumi preview should fail with "missing mandatory tag: Classification"

  # ---------------------------------------------------------------------------
  # REQ-INFRA-022: Bedrock PrivateLink endpoint connectivity verification
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-022
  Scenario: aegis verify --check endpoint --cloud aws confirms Bedrock reachable
    Given a provisioned AWS GovCloud boundary with PrivateLink
    When the user runs "aegis verify --check endpoint --cloud aws"
    Then the VPC endpoint state should be "available"
    And DNS resolution should succeed for the Bedrock endpoint
    And the security group should allow HTTPS (port 443)

  # @req REQ-INFRA-022
  Scenario: aegis verify --check endpoint --cloud aws fails when PrivateLink is down
    Given a provisioned AWS GovCloud boundary
    And the VPC endpoint has been manually deleted
    When the user runs "aegis verify --check endpoint --cloud aws"
    Then the check should FAIL with "Bedrock PrivateLink endpoint not found"

  # ---------------------------------------------------------------------------
  # REQ-INFRA-023: Azure Government infrastructure preview (dry-run)
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-023
  Scenario: aegis plan --cloud azure outputs resource diff without side effects
    Given valid Azure Government credentials
    When the user executes "aegis plan --cloud azure"
    Then the output should list all resources that would be created in Azure Gov
    And include Key Vault, VNet, Private Endpoint, Monitor, Storage Account
    And no resources should be created or modified in Azure

  # @req REQ-INFRA-023
  Scenario: aegis plan --cloud azure shows update diff for existing resources
    Given an existing Azure Gov boundary
    And a NSG rule has been manually modified
    When the user executes "aegis plan --cloud azure"
    Then the output should show the NSG as "update" with a diff

  # ---------------------------------------------------------------------------
  # REQ-INFRA-024: Azure Government drift detection
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-024
  Scenario: aegis drift --cloud azure reports resource drift
    Given an existing Azure Gov boundary
    And the Monitor diagnostic settings have been manually disabled
    When the user executes "aegis drift --cloud azure"
    Then the report should flag Monitor drift with severity "CRITICAL"

  # @req REQ-INFRA-024
  Scenario: aegis drift --cloud azure reports no drift when compliant
    Given an existing Azure Gov boundary with no manual modifications
    When the user executes "aegis drift --cloud azure"
    Then the report should indicate "No drift detected"

  # ---------------------------------------------------------------------------
  # REQ-INFRA-025: Azure Government teardown safety gate
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-025
  Scenario: aegis destroy --cloud azure requires typed subscription ID confirmation
    Given a provisioned Azure Gov boundary in subscription "sub-12345"
    When the user executes "aegis destroy --cloud azure"
    Then aegis should display the complete resource tree
    And prompt the user to type the subscription ID to confirm

  # @req REQ-INFRA-025
  Scenario: aegis destroy --cloud azure is blocked if Monitor is disabled
    Given a provisioned Azure Gov boundary
    And Azure Monitor has been manually disabled
    When the user executes "aegis destroy --cloud azure"
    Then aegis should refuse teardown with "Cannot destroy: Azure Monitor is disabled"

  # ---------------------------------------------------------------------------
  # REQ-INFRA-026: Azure Government mandatory compliance tags
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-026
  Scenario: All Azure resources carry required compliance tags
    Given a provisioned Azure Gov boundary
    When I inspect the tags on all provisioned resources
    Then each resource should have tags: environment, classification, owner, cost-center
    And the tag schema should match the GCP and AWS label schemas

  # @req REQ-INFRA-026
  Scenario: Azure Policy enforces mandatory tags on all resources
    Given Azure Policy is configured for the subscription
    When a resource is deployed without the "classification" tag
    Then Azure Policy should deny the deployment
    And Pulumi should report the policy violation

  # ---------------------------------------------------------------------------
  # REQ-INFRA-027: Azure OpenAI Private Endpoint connectivity verification
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-027
  Scenario: aegis verify --check endpoint --cloud azure confirms Azure OpenAI reachable
    Given a provisioned Azure Gov boundary with Private Endpoint
    When the user runs "aegis verify --check endpoint --cloud azure"
    Then the Private Endpoint connection state should be "Approved"
    And private IP DNS resolution should succeed
    And the Azure Gov CA should be in the trust chain

  # @req REQ-INFRA-027
  Scenario: aegis verify --check endpoint --cloud azure fails when endpoint is deleted
    Given a provisioned Azure Gov boundary
    And the Private Endpoint has been manually deleted
    When the user runs "aegis verify --check endpoint --cloud azure"
    Then the check should FAIL with "Azure OpenAI Private Endpoint not found"

  # ---------------------------------------------------------------------------
  # REQ-INFRA-028: Cross-cloud Pulumi state backend with unified encryption
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-028
  Scenario: All cloud stacks use a single encrypted state backend
    Given aegis manages infrastructure on GCP, AWS, and Azure
    When I inspect the Pulumi state backend configuration
    Then all three stacks should use the same state backend
    And the state should be envelope-encrypted with the provider's KMS

  # @req REQ-INFRA-028
  Scenario: State backend supports local, GCS, S3, and Azure Blob options
    Given the config sets state_backend = "s3"
    And valid S3 credentials are available
    When aegis stores Pulumi state
    Then the state should be written to the configured S3 bucket
    And the state should be encrypted at rest

  # @req REQ-INFRA-028
  Scenario: State backend falls back to local when no cloud backend is configured
    Given the config does not specify a state_backend
    When aegis stores Pulumi state
    Then the state should be written to "~/.aegis/pulumi/state/"
    And the state should be encrypted with a local passphrase

  # ---------------------------------------------------------------------------
  # REQ-INFRA-029: Cost estimation for AWS and Azure before provisioning
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-029
  Scenario: aegis plan --cloud aws --estimate shows monthly cost
    Given valid AWS GovCloud credentials
    When the user runs "aegis plan --cloud aws --estimate"
    Then the output should include estimated monthly cost for each AWS resource
    And GovCloud pricing premiums should be reflected

  # @req REQ-INFRA-029
  Scenario: aegis plan --cloud azure --estimate shows monthly cost
    Given valid Azure Government credentials
    When the user runs "aegis plan --cloud azure --estimate"
    Then the output should include estimated monthly cost for each Azure resource
    And Azure Government pricing premiums should be reflected

  # ---------------------------------------------------------------------------
  # REQ-INFRA-030: AWS GovCloud multi-region data-residency enforcement
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-030
  Scenario: Non-GovCloud region is rejected for AWS
    Given the user attempts "aegis init --cloud aws" with region "us-east-1"
    When the region validation runs
    Then aegis should reject with "Non-GovCloud region not permitted: us-east-1"
    And suggest "us-gov-west-1" or "us-gov-east-1"

  # @req REQ-INFRA-030
  Scenario: GovCloud region is accepted for AWS
    Given the user attempts "aegis init --cloud aws" with region "us-gov-west-1"
    When the region validation runs
    Then the region should be accepted and provisioning should proceed

  # ---------------------------------------------------------------------------
  # REQ-INFRA-031: Azure Government multi-region data-residency enforcement
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-031
  Scenario: Non-Gov region is rejected for Azure
    Given the user attempts "aegis init --cloud azure" with region "eastus"
    When the region validation runs
    Then aegis should reject with "Non-Government region not permitted: eastus"
    And suggest "usgovvirginia", "usgovtexas", "usgoviowa", or "usgovarizona"

  # @req REQ-INFRA-031
  Scenario: Azure Government region is accepted
    Given the user attempts "aegis init --cloud azure" with region "usgovvirginia"
    When the region validation runs
    Then the region should be accepted and provisioning should proceed

  # ---------------------------------------------------------------------------
  # REQ-INFRA-032: Cross-cloud NIST 800-171 compliance report
  # ---------------------------------------------------------------------------

  # @req REQ-INFRA-032
  Scenario: aegis verify --compliance --cloud all produces unified report
    Given infrastructure is provisioned on GCP, AWS, and Azure
    When the user runs "aegis verify --compliance --cloud all"
    Then the output should be a unified report keyed by NIST 800-171 control ID
    And each control should show per-cloud PASS/FAIL/WARN status
    And the overall status should require all clouds to pass

  # @req REQ-INFRA-032
  Scenario: Cross-cloud compliance fails when one cloud is non-compliant
    Given infrastructure is provisioned on GCP and AWS
    And the AWS VPC PrivateLink is missing
    When the user runs "aegis verify --compliance --cloud all"
    Then the AWS boundary protection control should show FAIL
    And the overall status should be "NON-COMPLIANT"
    And the GCP controls should still show their individual statuses
