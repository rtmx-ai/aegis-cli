//! aegis-audit: Immutable local audit ledger.
//!
//! Implements the `AuditLedger` port. Appends domain events as JSONL to
//! ~/.aegis/logs/*.jsonl. Records metadata only -- never CUI content.
//! Supports RTMX requirement linking via req_id in ledger entries.

pub mod cloud; // rtmx:req REQ-AUDIT-002
pub mod cost; // rtmx:req REQ-AUDIT-021a, REQ-AUDIT-021b
pub mod forwarding; // rtmx:req REQ-AUDIT-018, REQ-AUDIT-019, REQ-AUDIT-020
pub mod hash_chain; // rtmx:req REQ-AUDIT-005
pub mod ledger; // rtmx:req REQ-AUDIT-001
pub mod reconstruct; // rtmx:req REQ-AUDIT-017
pub mod redaction; // rtmx:req REQ-AUDIT-015
pub mod retention; // rtmx:req REQ-AUDIT-010
pub mod roi; // rtmx:req REQ-AUDIT-023a
pub mod rotation; // rtmx:req REQ-AUDIT-009
pub mod search; // rtmx:req REQ-AUDIT-013
pub mod siem; // rtmx:req REQ-AUDIT-011
pub mod syslog; // rtmx:req REQ-AUDIT-012
// pub mod evidence;  // rtmx:req REQ-AUDIT-003
