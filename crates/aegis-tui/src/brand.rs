//! Brand constants for the aegis TUI.
//!
//! All logos are pure ASCII and fit within 80-column terminals.

/// Full ASCII shield logo for the splash screen (8 lines).
///
/// Shield outline with `>` chevron inside, using the brand's `#`-heavy
/// character palette. Derived from the full-size generated art.
pub const LOGO_FULL: &str = "\
     .#########.
   /##...    ...##\\
  |##   \\      ##|
  |#     \\      #|
  |#      >     #|
  |#     /      #|
   \\##  /    ..##/
     -#########-";

/// Compact logo for the status bar. Fits in a tight prefix.
pub const LOGO_COMPACT: &str = "(>) aegis";

/// Brand promise shown below the logo on the splash screen.
pub const BRAND_PROMISE: &str = "Terminal-native AI pair programmer for CUI environments";

/// Returns the version string from Cargo at compile time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-TUI-031
    #[test]
    fn logo_full_fits_80_columns() {
        for line in LOGO_FULL.lines() {
            assert!(
                line.len() <= 80,
                "LOGO_FULL line exceeds 80 columns ({} chars): {:?}",
                line.len(),
                line,
            );
        }
    }

    // rtmx:req REQ-TUI-031
    #[test]
    fn logo_full_is_6_to_8_lines() {
        let count = LOGO_FULL.lines().count();
        assert!(
            (6..=8).contains(&count),
            "LOGO_FULL should be 6-8 lines, got {count}",
        );
    }

    // rtmx:req REQ-TUI-031
    #[test]
    fn logo_compact_fits_80_columns() {
        assert!(
            LOGO_COMPACT.len() <= 80,
            "LOGO_COMPACT exceeds 80 columns: {} chars",
            LOGO_COMPACT.len(),
        );
    }

    // rtmx:req REQ-TUI-031
    #[test]
    fn brand_promise_is_not_empty() {
        assert!(!BRAND_PROMISE.is_empty());
    }

    // rtmx:req REQ-TUI-031
    #[test]
    fn version_is_semver() {
        assert!(VERSION.contains('.'), "VERSION should be semver: {VERSION}",);
    }

    // rtmx:req REQ-TUI-031
    #[test]
    fn logo_full_is_pure_ascii() {
        assert!(LOGO_FULL.is_ascii(), "LOGO_FULL must be pure ASCII",);
    }

    // rtmx:req REQ-TUI-031
    #[test]
    fn logo_compact_is_pure_ascii() {
        assert!(LOGO_COMPACT.is_ascii(), "LOGO_COMPACT must be pure ASCII",);
    }
}
