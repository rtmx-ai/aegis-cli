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

## Traceability
- **Parent:** GEMINI.md Section 4
- **Tests:** `pkg/infra/boundary_test.go`
