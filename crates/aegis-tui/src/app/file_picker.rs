//! Interactive file picker for @-mention context injection.
//!
//! When the user types `@` in the input field, a modal overlay appears
//! listing files from the working directory. The user can fuzzy-filter,
//! navigate, and select a file to inject into the input as context.

use std::fs;
use std::path::Path;

/// Maximum directory traversal depth.
const MAX_DEPTH: usize = 3;

/// Maximum number of file entries to collect.
const MAX_ENTRIES: usize = 500;

/// Interactive file picker state.
#[derive(Debug, Clone)]
pub struct FilePicker {
    /// Fuzzy filter text typed by the user.
    pub query: String,
    /// All candidate file paths (relative to working directory).
    pub entries: Vec<String>,
    /// Entries matching the current query.
    pub filtered: Vec<String>,
    /// Index into `filtered` for the currently selected entry.
    pub selected: usize,
}

impl FilePicker {
    /// Create a new file picker with the given candidate entries.
    pub fn new(entries: Vec<String>) -> Self {
        let filtered = entries.clone();
        Self {
            query: String::new(),
            entries,
            filtered,
            selected: 0,
        }
    }

    /// Update the query and refilter entries. Resets selection to 0.
    pub fn update_query(&mut self, query: &str) {
        self.query = query.to_string();
        let lower = query.to_lowercase();
        self.filtered = if lower.is_empty() {
            self.entries.clone()
        } else {
            self.entries
                .iter()
                .filter(|e| e.to_lowercase().contains(&lower))
                .cloned()
                .collect()
        };
        self.selected = 0;
    }

    /// Get the currently selected file path, if any.
    pub fn selected_path(&self) -> Option<&str> {
        self.filtered.get(self.selected).map(|s| s.as_str())
    }

    /// Move selection to the next entry, wrapping around.
    pub fn select_next(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = (self.selected + 1) % self.filtered.len();
        }
    }

    /// Move selection to the previous entry, wrapping around.
    pub fn select_prev(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = if self.selected == 0 {
                self.filtered.len() - 1
            } else {
                self.selected - 1
            };
        }
    }

    /// Scan a directory recursively for file entries, respecting depth
    /// and entry count limits. Skips hidden directories and `target/`.
    pub fn scan_directory(root: &Path) -> Vec<String> {
        let mut entries = Vec::new();
        collect_files(root, root, 0, &mut entries);
        entries.sort();
        entries
    }
}

/// Recursively collect file paths relative to `root`.
fn collect_files(root: &Path, dir: &Path, depth: usize, out: &mut Vec<String>) {
    if depth > MAX_DEPTH || out.len() >= MAX_ENTRIES {
        return;
    }

    let read_dir = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };

    for entry in read_dir {
        if out.len() >= MAX_ENTRIES {
            return;
        }
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip hidden directories/files and target/
        if name_str.starts_with('.') || name_str == "target" {
            continue;
        }

        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, depth + 1, out);
        } else if path.is_file()
            && let Ok(relative) = path.strip_prefix(root)
        {
            // Normalize to forward slashes for cross-platform consistency.
            out.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // @req REQ-TUI-018
    #[test]
    fn new_populates_entries_and_filtered() {
        let entries = vec!["src/main.rs".to_string(), "Cargo.toml".to_string()];
        let picker = FilePicker::new(entries.clone());
        assert_eq!(picker.entries, entries);
        assert_eq!(picker.filtered, entries);
        assert_eq!(picker.selected, 0);
        assert_eq!(picker.query, "");
    }

    // @req REQ-TUI-018
    #[test]
    fn update_query_filters_case_insensitive() {
        let entries = vec![
            "src/main.rs".to_string(),
            "src/lib.rs".to_string(),
            "Cargo.toml".to_string(),
            "README.md".to_string(),
        ];
        let mut picker = FilePicker::new(entries);

        picker.update_query("cargo");
        assert_eq!(picker.filtered, vec!["Cargo.toml"]);
        assert_eq!(picker.selected, 0);

        picker.update_query("src");
        assert_eq!(picker.filtered, vec!["src/main.rs", "src/lib.rs"]);

        picker.update_query("");
        assert_eq!(picker.filtered.len(), 4);
    }

    // @req REQ-TUI-018
    #[test]
    fn selected_path_returns_current_selection() {
        let entries = vec!["a.rs".to_string(), "b.rs".to_string()];
        let picker = FilePicker::new(entries);
        assert_eq!(picker.selected_path(), Some("a.rs"));
    }

    // @req REQ-TUI-018
    #[test]
    fn selected_path_returns_none_when_empty() {
        let picker = FilePicker::new(Vec::new());
        assert_eq!(picker.selected_path(), None);
    }

    // @req REQ-TUI-018
    #[test]
    fn select_next_wraps_around() {
        let entries = vec!["a.rs".to_string(), "b.rs".to_string(), "c.rs".to_string()];
        let mut picker = FilePicker::new(entries);
        assert_eq!(picker.selected, 0);

        picker.select_next();
        assert_eq!(picker.selected, 1);

        picker.select_next();
        assert_eq!(picker.selected, 2);

        picker.select_next();
        assert_eq!(picker.selected, 0); // wrapped
    }

    // @req REQ-TUI-018
    #[test]
    fn select_prev_wraps_around() {
        let entries = vec!["a.rs".to_string(), "b.rs".to_string(), "c.rs".to_string()];
        let mut picker = FilePicker::new(entries);
        assert_eq!(picker.selected, 0);

        picker.select_prev();
        assert_eq!(picker.selected, 2); // wrapped to end

        picker.select_prev();
        assert_eq!(picker.selected, 1);
    }

    // @req REQ-TUI-018
    #[test]
    fn select_next_noop_on_empty() {
        let mut picker = FilePicker::new(Vec::new());
        picker.select_next(); // should not panic
        assert_eq!(picker.selected, 0);
    }

    // @req REQ-TUI-018
    #[test]
    fn select_prev_noop_on_empty() {
        let mut picker = FilePicker::new(Vec::new());
        picker.select_prev(); // should not panic
        assert_eq!(picker.selected, 0);
    }

    // @req REQ-TUI-018
    #[test]
    fn update_query_resets_selection() {
        let entries = vec!["a.rs".to_string(), "b.rs".to_string()];
        let mut picker = FilePicker::new(entries);
        picker.select_next(); // selected = 1
        picker.update_query("a");
        assert_eq!(picker.selected, 0);
    }

    // @req REQ-TUI-018
    #[test]
    fn scan_directory_finds_files() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("file1.rs"), "").unwrap();
        fs::write(tmp.path().join("file2.txt"), "").unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        fs::write(tmp.path().join("sub/nested.rs"), "").unwrap();

        let entries = FilePicker::scan_directory(tmp.path());
        assert!(entries.contains(&"file1.rs".to_string()));
        assert!(entries.contains(&"file2.txt".to_string()));
        assert!(entries.contains(&"sub/nested.rs".to_string()));
    }

    // @req REQ-TUI-018
    #[test]
    fn scan_directory_skips_hidden_and_target() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("visible.rs"), "").unwrap();
        fs::create_dir(tmp.path().join(".hidden")).unwrap();
        fs::write(tmp.path().join(".hidden/secret.rs"), "").unwrap();
        fs::create_dir(tmp.path().join("target")).unwrap();
        fs::write(tmp.path().join("target/debug.rs"), "").unwrap();

        let entries = FilePicker::scan_directory(tmp.path());
        assert!(entries.contains(&"visible.rs".to_string()));
        assert!(!entries.iter().any(|e| e.contains("hidden")));
        assert!(!entries.iter().any(|e| e.contains("target")));
    }

    // @req REQ-TUI-018
    #[test]
    fn scan_directory_respects_depth_limit() {
        let tmp = TempDir::new().unwrap();
        // Create depth 4: a/b/c/d/deep.rs -- should NOT appear
        let deep = tmp.path().join("a/b/c/d");
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("deep.rs"), "").unwrap();
        // Create depth 3: a/b/c/ok.rs -- should appear
        fs::write(tmp.path().join("a/b/c/ok.rs"), "").unwrap();

        let entries = FilePicker::scan_directory(tmp.path());
        assert!(entries.contains(&"a/b/c/ok.rs".to_string()));
        assert!(!entries.iter().any(|e| e.contains("deep.rs")));
    }
}
