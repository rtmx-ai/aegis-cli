//! Rendering for the slash command palette dropdown.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState};

use super::command_palette::CommandPaletteView;

/// Render the command palette as a floating dropdown above the input area.
pub fn render_command_palette(frame: &mut Frame, palette: &CommandPaletteView, input_area: Rect) {
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
            let line = Line::from(vec![
                Span::styled(
                    &entry.name,
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(&entry.description, Style::default().fg(Color::DarkGray)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Commands "))
        .highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White));

    let mut state = ListState::default();
    state.select(Some(palette.selected));
    frame.render_stateful_widget(list, palette_area, &mut state);
}
