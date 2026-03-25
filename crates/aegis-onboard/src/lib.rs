//! aegis-onboard: Initialization and onboarding state machine.
//!
//! Implements `aegis init` with deployment modes: Self-Service BYOC,
//! Enterprise BYOC, Managed SaaS, and Local (air-gapped).
//! Also manages ~/.aegis/config.yaml lifecycle.

pub mod config;
pub mod init;
