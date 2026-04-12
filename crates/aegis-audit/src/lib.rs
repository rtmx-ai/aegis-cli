//! aegis-audit: Immutable local audit ledger.
//!
//! Implements the `AuditLedger` port. Appends domain events as JSONL to
//! ~/.aegis/logs/*.jsonl. Records metadata only -- never CUI content.
//! Supports RTMX requirement linking via req_id in ledger entries.

pub mod cloud; // rtmx:req REQ-AUDIT-002
pub mod hash_chain; // rtmx:req REQ-AUDIT-005
pub mod ledger; // rtmx:req REQ-AUDIT-001
// pub mod evidence;  // rtmx:req REQ-AUDIT-003
