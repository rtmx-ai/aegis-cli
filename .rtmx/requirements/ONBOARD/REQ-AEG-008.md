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

## Traceability
- **Parent:** GEMINI.md Section 2
- **Tests:** `pkg/onboard/machine_test.go`
