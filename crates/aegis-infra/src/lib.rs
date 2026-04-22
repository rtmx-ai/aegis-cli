//! aegis-infra: Plugin host for the aegis-infra/v1 protocol.
//!
//! Spawns IaC plugin binaries as subprocesses and communicates via
//! NDJSON on stdout. Parses progress, diagnostic, check, and result
//! events. Aggregates health checks across plugins.

pub mod credentials;
pub mod events;
pub mod host;
pub mod mock_plugin;
pub mod outputs;
pub mod preview;
pub mod relay;
