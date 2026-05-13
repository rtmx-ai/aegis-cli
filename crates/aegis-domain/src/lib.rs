//! aegis-domain: Shared kernel for the Aegis CLI.
//!
//! This crate contains value objects, domain events, error types, and port
//! traits shared across bounded contexts. It has zero I/O dependencies.

pub mod dep_graph;
pub mod error;
pub mod event;
pub mod nist;
pub mod ports;
pub mod rtmx;
pub mod types;

pub use error::DomainError;
