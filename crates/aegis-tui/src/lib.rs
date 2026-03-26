//! aegis-tui: Terminal user interface built with ratatui.
//!
//! Single-pane Claude Code-inspired layout: status line (top), scrolling
//! chat log (fill), multi-line input (bottom). Inline tool calls, diffs,
//! and HITL approval dialogs within the chat flow.

pub mod input;
pub mod layout;
pub mod markdown;
pub mod messages;
pub mod slash_commands;
pub mod thinking;
