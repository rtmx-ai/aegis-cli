//! aegis-onboard: Initialization and onboarding state machine.
//!
//! Implements `aegis init` with deployment modes: Self-Service BYOC,
//! Enterprise BYOC, Managed SaaS, and Local (air-gapped).
//! Also manages ~/.aegis/config.yaml lifecycle.

pub mod adc;
pub mod backend_select;
pub mod byoc;
pub mod config;
pub mod connectivity;
pub mod credentials;
pub mod detect;
pub mod export;
pub mod gateway;
pub mod init;
pub mod migration;
pub mod mtls;
pub mod plugin_download;
pub mod plugin_preview;
pub mod service_token;
pub mod tutorial;
