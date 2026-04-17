//! aegis-hitl: Human-in-the-Loop approval gate.
//!
//! Implements the HITL boundary that blocks all state-mutating tool calls
//! until the user explicitly approves, denies, edits, or skips them.
//! Permission decisions are logged to the audit ledger.

pub mod approval;
pub mod batch;
pub mod gate;
pub mod grants;
pub mod kill_switch;
pub mod rollback;
pub mod rules;
