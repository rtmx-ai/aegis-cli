//! aegis-security: Security boundary enforcement.
//!
//! Implements the `SecurityFilter` port for .aegisignore context filtering.
//! Provides OS-level sandboxing (bubblewrap/seatbelt) for command execution.
//! Enforces TLS 1.3 with FIPS 140-2 validated cryptography.

pub mod aegisignore;

// pub mod sandbox;    // @req REQ-SECURITY-003
// pub mod transport;  // @req REQ-SECURITY-002
