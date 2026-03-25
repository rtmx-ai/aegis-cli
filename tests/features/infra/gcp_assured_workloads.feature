Feature: GCP Assured Workloads Provisioning
  As a defense engineer using Self-Service BYOC mode
  I need aegis init to provision a compliant GCP boundary
  So that my Vertex AI interactions satisfy NIST 800-171 and IL4/IL5

  # @req REQ-INFRA-001
  Scenario: Successful infrastructure provisioning
    Given valid GCP Application Default Credentials
    When "aegis init" provisions the GCP boundary
    Then a Cloud KMS CMEK key should be created with 30-day rotation
    And a VPC with Private Google Access should be created
    And VPC Service Controls should restrict aiplatform.googleapis.com
    And Cloud Audit Logs should be enabled for the Vertex AI service
    And audit logs should be routed to a CMEK-encrypted Storage bucket
    And the Vertex AI endpoint should be pinned to a specific model version

  # @req REQ-INFRA-001
  Scenario: Provisioning fails with missing credentials
    Given no GCP credentials on the workstation
    When the user executes "aegis init"
    Then the environment probe should detect missing credentials
    And display an error directing the user to run "gcloud auth application-default login"
    And no infrastructure should be provisioned

  # @req REQ-INFRA-001
  Scenario: Infrastructure teardown
    Given an existing Aegis GCP boundary
    When the user executes "aegis destroy"
    Then all GCP resources should be removed
    And the local config should reflect the removal
