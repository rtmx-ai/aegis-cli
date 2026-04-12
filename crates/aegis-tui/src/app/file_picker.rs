//! Interactive file picker for @-mention context injection.
//!
//! When the user types `@` in the input field, a dropdown appears below the
//! input listing files and directories from the resolved path. The picker is
//! path-aware: `@` alone scans cwd, `@src/` scans ./src/, `@/tmp/` scans
//! /tmp/, `@~/` scans $HOME.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Maximum number of directory entries to collect.
const MAX_ENTRIES: usize = 500;

/// A single directory entry (file or subdirectory).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    /// Display name. Directories include a trailing `/`.
    pub name: String,
    /// Whether this entry is a directory.
    pub is_dir: bool,
}

/// A flattened tree entry for rendering the tree view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    /// Display name (with trailing `/` for directories).
    pub name: String,
    /// Whether this entry is a directory.
    pub is_dir: bool,
    /// Nesting depth (0 = root level).
    pub depth: usize,
    /// Whether this directory is currently expanded (false for files).
    pub expanded: bool,
}

/// Interactive file picker state.
#[derive(Debug, Clone)]
pub struct FilePicker {
    /// Raw query text typed after `@` (e.g. "src/ma").
    pub query: String,
    /// Resolved base directory being scanned.
    pub base_dir: PathBuf,
    /// All entries in `base_dir` (non-hidden, one level).
    pub entries: Vec<DirEntry>,
    /// Entries matching the current filter portion of the query.
    pub filtered: Vec<DirEntry>,
    /// Index into `filtered` for the currently selected entry.
    pub selected: usize,
    /// Set of directory paths (relative to base_dir) that are expanded.
    pub expanded: HashSet<String>,
}

impl FilePicker {
    /// Open a new file picker by resolving the query against `cwd`.
    pub fn open(query: &str, cwd: &Path) -> Self {
        let (base_dir, filter) = resolve_path(query, cwd);
        let entries = scan_directory(&base_dir);
        let filtered = filter_entries(&entries, filter);
        Self {
            query: query.to_string(),
            base_dir,
            entries,
            filtered,
            selected: 0,
            expanded: HashSet::new(),
        }
    }

    /// Update the query. Re-scans only when the base directory changes.
    pub fn update_query(&mut self, query: &str, cwd: &Path) {
        let (new_base, filter) = resolve_path(query, cwd);
        self.query = query.to_string();
        if new_base != self.base_dir {
            self.base_dir = new_base;
            self.entries = scan_directory(&self.base_dir);
        }
        self.filtered = filter_entries(&self.entries, filter);
        self.selected = 0;
    }

    /// Get the currently selected entry, if any.
    pub fn selected_entry(&self) -> Option<&DirEntry> {
        self.filtered.get(self.selected)
    }

    /// Get the full path string for the selected entry, suitable for
    /// insertion into the input. For directories, the path ends with `/`.
    pub fn selected_path(&self) -> Option<String> {
        self.selected_entry().map(|e| {
            let mut path = self.query.to_string();
            // Strip the filter portion (text after last `/` or all if no `/`)
            if let Some(slash_pos) = path.rfind('/') {
                path.truncate(slash_pos + 1);
            } else {
                path.clear();
            }
            path.push_str(&e.name);
            path
        })
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

    /// Toggle the expanded state of the currently selected directory.
    /// If the selected entry is a file, this is a no-op.
    /// Returns `true` if a directory was toggled (caller should NOT
    /// treat Enter as "select file").
    pub fn toggle_expand(&mut self, _cwd: &Path) -> bool {
        let tree = self.tree_entries();
        if let Some(entry) = tree.get(self.selected) {
            if !entry.is_dir {
                return false;
            }
            let key = entry.name.clone();
            if self.expanded.contains(&key) {
                self.expanded.remove(&key);
            } else {
                self.expanded.insert(key);
            }
            true
        } else {
            false
        }
    }

    /// Build a flat list of visible tree entries respecting expanded state.
    /// Each entry carries its depth for indentation rendering.
    pub fn tree_entries(&self) -> Vec<TreeEntry> {
        let mut result = Vec::new();
        self.collect_tree_entries(&self.filtered, 0, "", &mut result);
        result
    }

    /// Recursively collect visible entries for the tree view.
    fn collect_tree_entries(
        &self,
        entries: &[DirEntry],
        depth: usize,
        parent_path: &str,
        result: &mut Vec<TreeEntry>,
    ) {
        for entry in entries {
            let full_path = if parent_path.is_empty() {
                entry.name.clone()
            } else {
                format!("{}{}", parent_path, entry.name)
            };

            let is_expanded = entry.is_dir && self.expanded.contains(&entry.name);

            result.push(TreeEntry {
                name: entry.name.clone(),
                is_dir: entry.is_dir,
                depth,
                expanded: is_expanded,
            });

            // If this directory is expanded, scan and add its children.
            if is_expanded {
                let child_dir = self.base_dir.join(entry.name.trim_end_matches('/'));
                let children = scan_directory(&child_dir);
                self.collect_tree_entries(&children, depth + 1, &full_path, result);
            }
        }
    }
}

/// Resolve an `@`-query into a (base_directory, filter_text) pair.
///
/// - `""` -> (cwd, "")
/// - `"src/"` -> (cwd/src, "")
/// - `"src/ma"` -> (cwd/src, "ma")
/// - `"/tmp/"` -> (/tmp, "")
/// - `"~/"` -> ($HOME, "")
/// - `"~/Downloads/"` -> ($HOME/Downloads, "")
pub fn resolve_path<'a>(query: &'a str, cwd: &Path) -> (PathBuf, &'a str) {
    // Normalize backslashes to forward slashes.
    // We work with the original str for the filter slice, so we detect
    // separator positions using both `\` and `/`.
    let normalized: String = query.replace('\\', "/");

    // Split at the last `/` to separate directory from filter.
    let (dir_part, filter) = match normalized.rfind('/') {
        Some(pos) => {
            // dir_part includes the trailing slash content
            let dir_part = &normalized[..=pos];
            let filter = &query[pos + 1..];
            (dir_part.to_string(), filter)
        }
        None => {
            // No slash -- everything is filter, base is cwd
            return (cwd.to_path_buf(), query);
        }
    };

    // Resolve the directory part.
    let base = if let Some(stripped) = dir_part.strip_prefix("~/") {
        let home = home_dir();
        let rest = stripped.trim_end_matches('/');
        if rest.is_empty() {
            home
        } else {
            home.join(rest)
        }
    } else if dir_part.starts_with('/') {
        PathBuf::from(&dir_part)
    } else {
        cwd.join(&dir_part)
    };

    (base, filter)
}

/// Scan a single directory level. Returns sorted entries, skipping hidden
/// names (starting with `.`). Directories have `is_dir: true` and their
/// name includes a trailing `/`.
pub fn scan_directory(dir: &Path) -> Vec<DirEntry> {
    let read_dir = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return Vec::new(),
    };

    let mut entries = Vec::new();
    for entry in read_dir {
        if entries.len() >= MAX_ENTRIES {
            break;
        }
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip hidden entries.
        if name_str.starts_with('.') {
            continue;
        }

        let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
        let display_name = if is_dir {
            format!("{}/", name_str)
        } else {
            // Normalize backslashes for cross-platform consistency.
            name_str.replace('\\', "/")
        };

        entries.push(DirEntry {
            name: display_name,
            is_dir,
        });
    }

    entries.sort_by(|a, b| {
        // Directories first, then alphabetical.
        b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name))
    });
    entries
}

/// Filter entries by a case-insensitive substring match on the name.
fn filter_entries(entries: &[DirEntry], filter: &str) -> Vec<DirEntry> {
    if filter.is_empty() {
        return entries.to_vec();
    }
    let lower = filter.to_lowercase();
    entries
        .iter()
        .filter(|e| e.name.to_lowercase().contains(&lower))
        .cloned()
        .collect()
}

/// Get the user's home directory.
fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // --- resolve_path tests ---

    // @req REQ-TUI-047
    #[test]
    fn resolve_path_bare_query_returns_cwd() {
        let cwd = Path::new("/projects/myapp");
        let (base, filter) = resolve_path("", cwd);
        assert_eq!(base, PathBuf::from("/projects/myapp"));
        assert_eq!(filter, "");
    }

    // @req REQ-TUI-047
    #[test]
    fn resolve_path_filter_only_no_slash() {
        let cwd = Path::new("/projects/myapp");
        let (base, filter) = resolve_path("main", cwd);
        assert_eq!(base, PathBuf::from("/projects/myapp"));
        assert_eq!(filter, "main");
    }

    // @req REQ-TUI-047
    #[test]
    fn resolve_path_relative_dir() {
        let cwd = Path::new("/projects/myapp");
        let (base, filter) = resolve_path("src/", cwd);
        assert_eq!(base, PathBuf::from("/projects/myapp/src/"));
        assert_eq!(filter, "");
    }

    // @req REQ-TUI-047
    #[test]
    fn resolve_path_relative_dir_with_filter() {
        let cwd = Path::new("/projects/myapp");
        let (base, filter) = resolve_path("src/ma", cwd);
        assert_eq!(base, PathBuf::from("/projects/myapp/src/"));
        assert_eq!(filter, "ma");
    }

    // @req REQ-TUI-047
    #[test]
    fn resolve_path_absolute() {
        let cwd = Path::new("/projects/myapp");
        let (base, filter) = resolve_path("/tmp/", cwd);
        assert_eq!(base, PathBuf::from("/tmp/"));
        assert_eq!(filter, "");
    }

    // @req REQ-TUI-047
    #[test]
    fn resolve_path_absolute_with_filter() {
        let cwd = Path::new("/projects/myapp");
        let (base, filter) = resolve_path("/tmp/foo", cwd);
        assert_eq!(base, PathBuf::from("/tmp/"));
        assert_eq!(filter, "foo");
    }

    // @req REQ-TUI-047
    #[test]
    fn resolve_path_home() {
        let cwd = Path::new("/projects/myapp");
        let (base, filter) = resolve_path("~/", cwd);
        let home = home_dir();
        assert_eq!(base, home);
        assert_eq!(filter, "");
    }

    // @req REQ-TUI-047
    #[test]
    fn resolve_path_home_subdir() {
        let cwd = Path::new("/projects/myapp");
        let (base, filter) = resolve_path("~/Downloads/", cwd);
        let home = home_dir();
        assert_eq!(base, home.join("Downloads"));
        assert_eq!(filter, "");
    }

    // @req REQ-TUI-047
    #[test]
    fn resolve_path_home_subdir_with_filter() {
        let cwd = Path::new("/projects/myapp");
        let (base, filter) = resolve_path("~/Downloads/re", cwd);
        let home = home_dir();
        assert_eq!(base, home.join("Downloads"));
        assert_eq!(filter, "re");
    }

    // --- FilePicker::open tests ---

    // @req REQ-TUI-047
    #[test]
    fn open_scans_correct_directory() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("hello.rs"), "").unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();

        let picker = FilePicker::open("", tmp.path());
        assert_eq!(picker.base_dir, tmp.path());
        // Should find both the file and directory
        let names: Vec<&str> = picker.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"hello.rs"), "entries: {names:?}");
        assert!(names.contains(&"sub/"), "entries: {names:?}");
    }

    // @req REQ-TUI-047
    #[test]
    fn open_scans_subdirectory() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("src");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("main.rs"), "").unwrap();
        fs::write(sub.join("lib.rs"), "").unwrap();

        let picker = FilePicker::open("src/", tmp.path());
        let names: Vec<&str> = picker.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"main.rs"), "entries: {names:?}");
        assert!(names.contains(&"lib.rs"), "entries: {names:?}");
    }

    // @req REQ-TUI-047
    #[test]
    fn open_with_filter_filters_entries() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("src");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("main.rs"), "").unwrap();
        fs::write(sub.join("lib.rs"), "").unwrap();

        let picker = FilePicker::open("src/ma", tmp.path());
        assert_eq!(picker.filtered.len(), 1);
        assert_eq!(picker.filtered[0].name, "main.rs");
    }

    // --- update_query tests ---

    // @req REQ-TUI-047
    #[test]
    fn update_query_rescans_on_dir_change() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("root.rs"), "").unwrap();
        let sub = tmp.path().join("src");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("child.rs"), "").unwrap();

        let mut picker = FilePicker::open("", tmp.path());
        assert!(picker.entries.iter().any(|e| e.name == "root.rs"));

        picker.update_query("src/", tmp.path());
        assert!(picker.entries.iter().any(|e| e.name == "child.rs"));
        assert!(!picker.entries.iter().any(|e| e.name == "root.rs"));
    }

    // @req REQ-TUI-047
    #[test]
    fn update_query_filters_without_rescan() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("src");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("main.rs"), "").unwrap();
        fs::write(sub.join("lib.rs"), "").unwrap();

        let mut picker = FilePicker::open("src/", tmp.path());
        assert_eq!(picker.filtered.len(), 2);

        picker.update_query("src/ma", tmp.path());
        assert_eq!(picker.filtered.len(), 1);
        assert_eq!(picker.filtered[0].name, "main.rs");
        // entries should still have both (no rescan)
        assert_eq!(picker.entries.len(), 2);
    }

    // --- Directory display tests ---

    // @req REQ-TUI-047
    #[test]
    fn directories_shown_with_trailing_slash() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("mydir")).unwrap();
        fs::write(tmp.path().join("myfile.txt"), "").unwrap();

        let entries = scan_directory(tmp.path());
        let dir_entry = entries.iter().find(|e| e.name.contains("mydir")).unwrap();
        assert!(dir_entry.is_dir);
        assert!(dir_entry.name.ends_with('/'));

        let file_entry = entries.iter().find(|e| e.name.contains("myfile")).unwrap();
        assert!(!file_entry.is_dir);
        assert!(!file_entry.name.ends_with('/'));
    }

    // --- Hidden entry tests ---

    // @req REQ-TUI-047
    #[test]
    fn hidden_entries_skipped() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("visible.rs"), "").unwrap();
        fs::create_dir(tmp.path().join(".hidden")).unwrap();
        fs::write(tmp.path().join(".gitignore"), "").unwrap();

        let entries = scan_directory(tmp.path());
        assert!(entries.iter().any(|e| e.name == "visible.rs"));
        assert!(!entries.iter().any(|e| e.name.contains("hidden")));
        assert!(!entries.iter().any(|e| e.name.contains("gitignore")));
    }

    // --- Navigation tests ---

    // @req REQ-TUI-047
    #[test]
    fn select_next_wraps_around() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.rs"), "").unwrap();
        fs::write(tmp.path().join("b.rs"), "").unwrap();

        let mut picker = FilePicker::open("", tmp.path());
        assert_eq!(picker.selected, 0);
        let len = picker.filtered.len();

        for _ in 0..len {
            picker.select_next();
        }
        assert_eq!(picker.selected, 0); // wrapped
    }

    // @req REQ-TUI-047
    #[test]
    fn select_prev_wraps_around() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.rs"), "").unwrap();
        fs::write(tmp.path().join("b.rs"), "").unwrap();

        let mut picker = FilePicker::open("", tmp.path());
        picker.select_prev();
        assert_eq!(picker.selected, picker.filtered.len() - 1);
    }

    // @req REQ-TUI-047
    #[test]
    fn selected_path_includes_query_prefix() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("src");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("main.rs"), "").unwrap();

        let picker = FilePicker::open("src/ma", tmp.path());
        let path = picker.selected_path().unwrap();
        assert_eq!(path, "src/main.rs");
    }

    // @req REQ-TUI-047
    #[test]
    fn selected_path_bare_query() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("hello.rs"), "").unwrap();

        let picker = FilePicker::open("", tmp.path());
        let path = picker.selected_path().unwrap();
        assert_eq!(path, "hello.rs");
    }

    // --- Sorting tests ---

    // @req REQ-TUI-047
    #[test]
    fn directories_sorted_before_files() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("aaa.rs"), "").unwrap();
        fs::create_dir(tmp.path().join("zzz")).unwrap();

        let entries = scan_directory(tmp.path());
        // Directory should come first even though 'z' > 'a'
        assert!(entries[0].is_dir, "First entry should be a directory");
        assert!(!entries[1].is_dir, "Second entry should be a file");
    }

    // --- Tree view tests ---

    // @req REQ-TUI-046
    #[test]
    fn tree_entries_returns_root_entries_when_nothing_expanded() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("hello.rs"), "").unwrap();
        fs::create_dir(tmp.path().join("src")).unwrap();

        let picker = FilePicker::open("", tmp.path());
        let tree = picker.tree_entries();

        assert_eq!(tree.len(), 2);
        // All at depth 0
        for entry in &tree {
            assert_eq!(entry.depth, 0, "Root entries should be depth 0");
        }
        // Directory should not be expanded
        let dir_entry = tree.iter().find(|e| e.is_dir).unwrap();
        assert!(!dir_entry.expanded);
    }

    // @req REQ-TUI-046
    #[test]
    fn toggle_expand_on_dir_adds_children() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("src")).unwrap();
        fs::write(tmp.path().join("src").join("main.rs"), "").unwrap();
        fs::write(tmp.path().join("src").join("lib.rs"), "").unwrap();
        fs::write(tmp.path().join("hello.rs"), "").unwrap();

        let mut picker = FilePicker::open("", tmp.path());
        // First entry should be the directory (dirs sort first)
        assert_eq!(picker.selected, 0);
        let tree_before = picker.tree_entries();
        assert_eq!(tree_before.len(), 2); // src/ and hello.rs

        picker.toggle_expand(tmp.path());

        let tree_after = picker.tree_entries();
        // Should now have src/ + 2 children + hello.rs = 4
        assert_eq!(tree_after.len(), 4);
        // First entry is expanded dir
        assert!(tree_after[0].is_dir);
        assert!(tree_after[0].expanded);
        assert_eq!(tree_after[0].depth, 0);
        // Children at depth 1
        assert_eq!(tree_after[1].depth, 1);
        assert_eq!(tree_after[2].depth, 1);
        // Last entry is the root file at depth 0
        assert_eq!(tree_after[3].depth, 0);
        assert_eq!(tree_after[3].name, "hello.rs");
    }

    // @req REQ-TUI-046
    #[test]
    fn toggle_expand_again_collapses_hides_children() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("src")).unwrap();
        fs::write(tmp.path().join("src").join("main.rs"), "").unwrap();
        fs::write(tmp.path().join("hello.rs"), "").unwrap();

        let mut picker = FilePicker::open("", tmp.path());
        // Expand
        picker.toggle_expand(tmp.path());
        let tree = picker.tree_entries();
        assert_eq!(tree.len(), 3); // src/ + main.rs child + hello.rs

        // Collapse
        picker.toggle_expand(tmp.path());
        let tree = picker.tree_entries();
        assert_eq!(tree.len(), 2); // back to src/ + hello.rs
        assert!(!tree[0].expanded);
    }

    // @req REQ-TUI-046
    #[test]
    fn tree_entries_have_correct_depth_values() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        fs::create_dir(&src).unwrap();
        let inner = src.join("inner");
        fs::create_dir(&inner).unwrap();
        fs::write(inner.join("deep.rs"), "").unwrap();
        fs::write(src.join("top.rs"), "").unwrap();
        fs::write(tmp.path().join("root.rs"), "").unwrap();

        let mut picker = FilePicker::open("", tmp.path());
        // Expand src/
        picker.toggle_expand(tmp.path());

        let tree = picker.tree_entries();
        // src/ (d0), inner/ (d1), top.rs (d1), root.rs (d0)
        assert_eq!(tree[0].depth, 0);
        assert_eq!(tree[0].name, "src/");

        // Find inner/ at depth 1
        let inner_entry = tree.iter().find(|e| e.name == "inner/").unwrap();
        assert_eq!(inner_entry.depth, 1);

        // Find top.rs at depth 1
        let top = tree.iter().find(|e| e.name == "top.rs").unwrap();
        assert_eq!(top.depth, 1);

        // root.rs at depth 0
        let root = tree.iter().find(|e| e.name == "root.rs").unwrap();
        assert_eq!(root.depth, 0);
    }

    // @req REQ-TUI-046
    #[test]
    fn toggle_expand_on_file_returns_false() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("hello.rs"), "").unwrap();

        let mut picker = FilePicker::open("", tmp.path());
        // Only entry is a file
        let toggled = picker.toggle_expand(tmp.path());
        assert!(!toggled, "toggle_expand on a file should return false");
    }
}
