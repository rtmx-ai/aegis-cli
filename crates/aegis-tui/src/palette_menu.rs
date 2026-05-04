//! Type selector menu for the bare `@` trigger.
//!
//! When the user types a bare `@` without any following text, a menu appears
//! offering quick selection of the context palette type: Files, Git changes,
//! Requirements, URL, or Symbols. A single keypress selects the type and
//! opens the appropriate picker.

/// The type of context palette to open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteType {
    Files,
    Git,
    Requirements,
    Url,
    Symbols,
}

/// The palette type selector menu shown on bare `@` trigger.
#[derive(Debug, Clone)]
pub struct PaletteMenu {
    /// Whether the menu is currently visible.
    pub visible: bool,
}

impl PaletteMenu {
    /// Create a new palette menu (initially hidden).
    pub fn new() -> Self {
        Self { visible: false }
    }
}

impl Default for PaletteMenu {
    fn default() -> Self {
        Self::new()
    }
}

/// Return the list of palette types with their hotkeys and labels.
pub fn palette_types() -> Vec<(char, PaletteType, &'static str)> {
    vec![
        ('f', PaletteType::Files, "Files"),
        ('g', PaletteType::Git, "Git changes"),
        ('r', PaletteType::Requirements, "Requirements"),
        ('u', PaletteType::Url, "URL"),
        ('s', PaletteType::Symbols, "Symbols"),
    ]
}

/// Map a keypress character to a palette type. Returns `None` for
/// unrecognized keys.
pub fn select_palette_type(key: char) -> Option<PaletteType> {
    match key {
        'f' => Some(PaletteType::Files),
        'g' => Some(PaletteType::Git),
        'r' => Some(PaletteType::Requirements),
        'u' => Some(PaletteType::Url),
        's' => Some(PaletteType::Symbols),
        _ => None,
    }
}

/// Render the type selector menu as a formatted string.
pub fn format_menu() -> String {
    let types = palette_types();
    let entries: Vec<String> = types
        .iter()
        .map(|(key, _, label)| format!("[{key}] {label}"))
        .collect();
    entries.join("  ")
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-TUI-054
    #[test]
    fn test_bare_at_shows_type_menu() {
        let menu = format_menu();
        assert!(menu.contains("Files"), "menu should list Files: {menu}");
        assert!(menu.contains("Git"), "menu should list Git: {menu}");
        assert!(
            menu.contains("Requirements"),
            "menu should list Requirements: {menu}"
        );
        assert!(menu.contains("URL"), "menu should list URL: {menu}");
        assert!(menu.contains("Symbols"), "menu should list Symbols: {menu}");
    }

    // rtmx:req REQ-TUI-054
    #[test]
    fn test_select_palette_type_f() {
        assert_eq!(select_palette_type('f'), Some(PaletteType::Files));
    }

    // rtmx:req REQ-TUI-054
    #[test]
    fn test_select_palette_type_r() {
        assert_eq!(select_palette_type('r'), Some(PaletteType::Requirements));
    }

    // rtmx:req REQ-TUI-054
    #[test]
    fn test_select_palette_type_invalid() {
        assert_eq!(select_palette_type('x'), None);
    }

    // rtmx:req REQ-TUI-054
    #[test]
    fn test_palette_types_has_five_entries() {
        let types = palette_types();
        assert_eq!(types.len(), 5);
    }
}
