//! File-path detection for pasted clipboard text.
//!
//! arboard v3 does not expose a `get_files()` API. Instead, this module
//! detects pasted text that looks like filesystem paths (lines starting
//! with `/`, `~/`, or `./`) and validates whether those paths exist on
//! disk. The caller can then offer to attach the files as context.

use std::path::PathBuf;

/// Parse pasted text for lines that look like file paths.
///
/// A line is considered a candidate path if, after trimming whitespace,
/// it starts with `/`, `~/`, or `./`. Each candidate is returned as a
/// `PathBuf` with `~` expanded to the home directory when possible.
pub fn detect_pasted_paths(text: &str) -> Vec<PathBuf> {
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return None;
            }
            if trimmed.starts_with('/') || trimmed.starts_with("~/") || trimmed.starts_with("./")
            {
                Some(expand_tilde(trimmed))
            } else {
                None
            }
        })
        .collect()
}

/// Validate a list of paths, splitting them into existing and missing.
///
/// Returns `(existing, missing)` where each element preserves the
/// original `PathBuf` from the input.
pub fn validate_paths(paths: &[PathBuf]) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut existing = Vec::new();
    let mut missing = Vec::new();
    for p in paths {
        if p.exists() {
            existing.push(p.clone());
        } else {
            missing.push(p.clone());
        }
    }
    (existing, missing)
}

/// Expand a leading `~` to the user's home directory.
fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Ok(home) = std::env::var("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-TUI-036
    #[test]
    fn test_file_paste_detects_paths() {
        let text = "/tmp/foo.rs\n/tmp/bar.rs";
        let paths = detect_pasted_paths(text);
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], PathBuf::from("/tmp/foo.rs"));
        assert_eq!(paths[1], PathBuf::from("/tmp/bar.rs"));
    }

    // rtmx:req REQ-TUI-036
    #[test]
    fn test_file_paste_detects_relative_and_tilde() {
        let text = "./src/main.rs\n~/Documents/readme.txt";
        let paths = detect_pasted_paths(text);
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], PathBuf::from("./src/main.rs"));
        // Tilde is expanded if HOME is set
        if let Ok(home) = std::env::var("HOME") {
            assert_eq!(paths[1], PathBuf::from(home).join("Documents/readme.txt"));
        }
    }

    // rtmx:req REQ-TUI-036
    #[test]
    fn test_file_paste_rejects_nonexistent() {
        let paths = vec![
            PathBuf::from("/tmp/__aegis_test_nonexistent_12345__"),
            PathBuf::from("/tmp"),
        ];
        let (existing, missing) = validate_paths(&paths);
        assert_eq!(existing.len(), 1);
        assert_eq!(existing[0], PathBuf::from("/tmp"));
        assert_eq!(missing.len(), 1);
        assert!(missing[0].to_str().unwrap().contains("nonexistent"));
    }

    // rtmx:req REQ-TUI-036
    #[test]
    fn test_file_paste_ignores_non_paths() {
        let text = "hello world\nthis is regular text\n42\n";
        let paths = detect_pasted_paths(text);
        assert!(paths.is_empty());
    }

    // rtmx:req REQ-TUI-036
    #[test]
    fn test_file_paste_handles_blank_lines() {
        let text = "/tmp/foo.rs\n\n\n/tmp/bar.rs\n";
        let paths = detect_pasted_paths(text);
        assert_eq!(paths.len(), 2);
    }

    // rtmx:req REQ-TUI-036
    #[test]
    fn test_file_paste_trims_whitespace() {
        let text = "  /tmp/foo.rs  \n  ./bar.rs  ";
        let paths = detect_pasted_paths(text);
        assert_eq!(paths.len(), 2);
    }
}
