//! aegis-onboard: Initialization and onboarding state machine.
//!
//! Implements `aegis init` with deployment modes: Self-Service BYOC,
//! Enterprise BYOC, Managed SaaS, and Local (air-gapped).
//! Also manages ~/.aegis/config.yaml lifecycle.

pub mod byoc;
pub mod config;
pub mod connectivity;
pub mod credentials;
pub mod detect;
pub mod init;
pub mod plugin_download;
pub mod service_token;
