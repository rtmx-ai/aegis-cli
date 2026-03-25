//! aegis-onboard: Initialization and onboarding state machine.
//!
//! Implements `aegis init` with three deployment modes: Self-Service BYOC,
//! Enterprise BYOC, and Managed SaaS. Also supports `aegis init --local`
//! for air-gapped operation.

// pub mod state_machine; // @req REQ-ONBOARD-001
// pub mod airgap;        // @req REQ-ONBOARD-003
