//! Mouse scroll handling.

use super::{Action, App};
use crossterm::event::{MouseEvent, MouseEventKind};

impl App {
    /// Lines scrolled per mouse wheel tick.
    pub(crate) const MOUSE_SCROLL_LINES: u16 = 1;

    pub(crate) fn handle_mouse(&mut self, mouse: MouseEvent) -> Action {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.scroll_offset = self.scroll_offset.saturating_add(Self::MOUSE_SCROLL_LINES);
                self.auto_scroll = false;
                Action::Continue
            }
            MouseEventKind::ScrollDown => {
                self.scroll_offset = self.scroll_offset.saturating_sub(Self::MOUSE_SCROLL_LINES);
                if self.scroll_offset == 0 {
                    self.auto_scroll = true;
                }
                Action::Continue
            }
            _ => Action::Continue,
        }
    }
}
