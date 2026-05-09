//! aegis-security: Security boundary enforcement.
//!
//! Implements the `SecurityFilter` port for .aegisignore context filtering.
//! Provides OS-level sandboxing (bubblewrap/seatbelt) for command execution.
//! Enforces TLS 1.3 with FIPS 140-2 validated cryptography (aws-lc-rs, CMVP #4631).

pub mod adversary;
pub mod aegisignore;
pub mod cert_pin;
pub mod cui;
pub mod dlp;
pub mod injection;
pub mod transport;

pub mod keychain;
pub mod sandbox;
