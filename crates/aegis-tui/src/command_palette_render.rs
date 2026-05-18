//! Rendering for the slash command palette dropdown.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState};

use super::command_palette::CommandPaletteView;
use crate::theme::Theme;

/// Render the command palette as a floating dropdown above the input area.
pub fn render_command_palette(
    frame: &mut Frame,
    palette: &CommandPaletteView,
    input_area: Rect,
    theme: &Theme,
) {
    let entries = &palette.entries;
    let height = (entries.len() as u16 + 2).min(12);

    let palette_area = Rect {
        x: input_area.x,
        y: input_area.y.saturating_sub(height),
        width: input_area.width.min(60),
        height,
    };

    frame.render_widget(Clear, palette_area);

    let items: Vec<ListItem> = entries
        .iter()
        .map(|entry| {
            // REQ-TUI-107: Restricted models rendered with dim style.
            let is_restricted = entry.description.contains("restricted");
            let (name_style, desc_style) = if is_restricted {
                (
                    Style::default()
                        .fg(theme.border)
                        .add_modifier(Modifier::DIM),
                    Style::default()
                        .fg(theme.border)
                        .add_modifier(Modifier::DIM),
                )
            } else {
                (
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                    Style::default().fg(theme.border),
                )
            };
            let line = Line::from(vec![
                Span::styled(&entry.name, name_style),
                Span::raw("  "),
                Span::styled(&entry.description, desc_style),
            ]);
            ListItem::new(line)
        })
        .collect();

    let title = match &palette.stage_hint {
        Some(hint) => format!(" {} ", hint),
        None => " Commands ".to_string(),
    };
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().bg(theme.status_bg).fg(theme.fg));

    let mut state = ListState::default();
    state.select(Some(palette.selected));
    frame.render_stateful_widget(list, palette_area, &mut state);
}
