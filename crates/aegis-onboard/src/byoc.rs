//! Enterprise BYOC mode detection and gateway URL resolution.
//!
//! Detects enterprise environments by checking:
//! 1. `AEGIS_GATEWAY_URL` environment variable
//! 2. `/etc/aegis/gateway.conf` system-wide config
//! 3. `~/.aegis/gateway.conf` user-level config
//!
//! When detected, stores `mode: enterprise-byoc` with `gateway_url` in config.

use std::path::{Path, PathBuf};

/// Detect whether the current environment is an enterprise BYOC setup.
///
/// Returns the gateway URL if any of these sources provide one:
/// 1. `AEGIS_GATEWAY_URL` environment variable (highest priority)
/// 2. `/etc/aegis/gateway.conf` (system-wide)
/// 3. `~/.aegis/gateway.conf` (user-level)
///
/// The first non-empty value wins. Returns `None` if no enterprise
/// environment is detected.
pub fn detect_byoc_environment() -> Option<String> {
    detect_byoc_with_home(dirs_home())
}

/// Testable variant that accepts an explicit home directory.
pub fn detect_byoc_with_home(home: Option<PathBuf>) -> Option<String> {
    // 1. Environment variable (highest priority)
    if let Some(url) = non_empty_env("AEGIS_GATEWAY_URL") {
        return Some(url);
    }

    // 2. System-wide config
    if let Some(url) = read_gateway_conf(Path::new("/etc/aegis/gateway.conf")) {
        return Some(url);
    }

    // 3. User-level config
    if let Some(home) = home {
        let user_conf = home.join(".aegis").join("gateway.conf");
        if let Some(url) = read_gateway_conf(&user_conf) {
            return Some(url);
        }
    }

    None
}

/// Read a gateway URL from a simple key=value config file.
///
/// Expects a file with lines like:
/// ```text
/// gateway_url=https://gateway.example.com
/// ```
/// Ignores comments (lines starting with `#`) and blank lines.
fn read_gateway_conf(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("gateway_url=") {
            let url = value.trim();
            if !url.is_empty() {
                return Some(url.to_string());
            }
        }
    }
    None
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // rtmx:req REQ-ONBOARD-017
    #[test]
    fn read_gateway_conf_ignores_comments_and_blanks() {
        let tmp = TempDir::new().unwrap();
        let conf = tmp.path().join("gateway.conf");
        std::fs::write(
            &conf,
            "# comment\n\n  \ngateway_url=https://gw.example.com\n",
        )
        .unwrap();

        let result = read_gateway_conf(&conf);
        assert_eq!(result, Some("https://gw.example.com".to_string()));
    }

    // rtmx:req REQ-ONBOARD-017
    #[test]
    fn read_gateway_conf_returns_none_for_missing_file() {
        let result = read_gateway_conf(Path::new("/nonexistent/gateway.conf"));
        assert!(result.is_none());
    }

    // rtmx:req REQ-ONBOARD-017
    #[test]
    fn read_gateway_conf_returns_none_for_empty_value() {
        let tmp = TempDir::new().unwrap();
        let conf = tmp.path().join("gateway.conf");
        std::fs::write(&conf, "gateway_url=\n").unwrap();

        let result = read_gateway_conf(&conf);
        assert!(result.is_none(), "Empty gateway_url value should be None");
    }

    // rtmx:req REQ-ONBOARD-017
    //
    // All scenarios that touch AEGIS_GATEWAY_URL run in a single test
    // to avoid race conditions from parallel tests mutating
    // process-wide environment variables.
    #[test]
    fn detect_byoc_scenarios() {
        fn clear() {
            unsafe { std::env::remove_var("AEGIS_GATEWAY_URL") };
        }

        // -- returns None when no sources --
        clear();
        let result = detect_byoc_with_home(Some(PathBuf::from("/nonexistent")));
        assert!(
            result.is_none(),
            "Should return None when no enterprise env detected"
        );

        // -- detects from env var --
        clear();
        unsafe {
            std::env::set_var("AEGIS_GATEWAY_URL", "https://gateway.corp.example.com");
        }
        let result = detect_byoc_with_home(Some(PathBuf::from("/nonexistent")));
        assert_eq!(result, Some("https://gateway.corp.example.com".to_string()));

        // -- detects from user config file --
        clear();
        let tmp_f = TempDir::new().unwrap();
        let aegis_dir_f = tmp_f.path().join(".aegis");
        std::fs::create_dir_all(&aegis_dir_f).unwrap();
        std::fs::write(
            aegis_dir_f.join("gateway.conf"),
            "# Enterprise gateway\ngateway_url=https://gw.internal.mil\n",
        )
        .unwrap();
        let result = detect_byoc_with_home(Some(tmp_f.path().to_path_buf()));
        assert_eq!(result, Some("https://gw.internal.mil".to_string()));

        // -- env var takes priority over file --
        let tmp = TempDir::new().unwrap();
        let aegis_dir = tmp.path().join(".aegis");
        std::fs::create_dir_all(&aegis_dir).unwrap();
        std::fs::write(
            aegis_dir.join("gateway.conf"),
            "gateway_url=https://from-file.example.com\n",
        )
        .unwrap();
        unsafe {
            std::env::set_var("AEGIS_GATEWAY_URL", "https://from-env.example.com");
        }
        let result = detect_byoc_with_home(Some(tmp.path().to_path_buf()));
        assert_eq!(
            result,
            Some("https://from-env.example.com".to_string()),
            "Env var should take priority over config file"
        );

        // -- ignores empty env var --
        clear();
        unsafe { std::env::set_var("AEGIS_GATEWAY_URL", "") };
        let result = detect_byoc_with_home(Some(PathBuf::from("/nonexistent")));
        assert!(result.is_none(), "Empty env var should be ignored");

        // Final cleanup
        clear();
    }
}
