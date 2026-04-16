//! aegis-tui: Terminal user interface built with ratatui.
//!
//! Single-pane Claude Code-inspired layout: status line (top), scrolling
//! chat log (fill), multi-line input (bottom). Inline tool calls, diffs,
//! and HITL approval dialogs within the chat flow.

pub mod app;
pub mod brand;
pub mod clipboard;
pub mod command_palette;
pub mod command_palette_render;
pub mod diff;
pub mod event;
pub mod input;
pub mod layout;
pub mod markdown;
pub mod messages;
pub mod platform;
pub mod slash_commands;
pub mod splash;
pub mod terminal;
pub mod thinking;
