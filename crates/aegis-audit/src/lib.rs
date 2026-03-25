//! aegis-audit: Immutable local audit ledger.
//!
//! Implements the `AuditLedger` port. Appends domain events as JSONL to
//! ~/.aegis/logs/*.jsonl. Records metadata only -- never CUI content.
//! Supports RTMX requirement linking via req_id in ledger entries.

pub mod ledger; // @req REQ-AUDIT-001
// pub mod evidence;  // @req REQ-AUDIT-003
