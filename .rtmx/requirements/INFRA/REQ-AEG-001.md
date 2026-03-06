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

## Traceability
- **Parent:** GEMINI.md Section 4
- **Tests:** `pkg/infra/pulumi_test.go`
