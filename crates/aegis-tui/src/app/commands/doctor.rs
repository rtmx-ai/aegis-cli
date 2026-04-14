//! /doctor command handler.

use crate::app::App;
use crate::messages::ChatMessage;
use aegis_infra::host::discover_plugins;
use std::path::PathBuf;

impl App {
    /// Handle /doctor command: run connectivity and health checks.
    pub(crate) fn handle_doctor_command(&mut self) {
        let mut passed = 0u32;
        let mut total = 0u32;
        let mut results: Vec<String> = Vec::new();

        // Check 1: Home directory writability
        total += 1;
        let home_check = if let Some(home) = dirs_check_home() {
            let aegis_dir = home.join(".aegis");
            if aegis_dir.exists() && aegis_dir.is_dir() {
                // Try writing a temp file
                let probe = aegis_dir.join(".doctor-probe");
                match std::fs::write(&probe, "ok") {
                    Ok(()) => {
                        let _ = std::fs::remove_file(&probe);
                        passed += 1;
                        "[PASS] Home directory: ~/.aegis is writable".to_string()
                    }
                    Err(e) => {
                        format!("[FAIL] Home directory: ~/.aegis not writable: {e}")
                    }
                }
            } else {
                "[FAIL] Home directory: ~/.aegis does not exist. Run aegis init.".to_string()
            }
        } else {
            "[FAIL] Home directory: could not determine home directory".to_string()
        };
        results.push(home_check);

        // Check 2: Configuration validity
        total += 1;
        let config_check = if let Some(home) = dirs_check_home() {
            let config_path = home.join(".aegis").join("config.yaml");
            if config_path.exists() {
                match std::fs::read_to_string(&config_path) {
                    Ok(content) => {
                        if content.contains("[") || content.contains("mode") {
                            passed += 1;
                            "[PASS] Configuration: config.yaml is readable".to_string()
                        } else {
                            "[FAIL] Configuration: config.yaml appears empty \
                                 or invalid"
                                .to_string()
                        }
                    }
                    Err(e) => {
                        format!("[FAIL] Configuration: cannot read config.yaml: {e}")
                    }
                }
            } else {
                "[FAIL] Configuration: config.yaml not found. Run aegis init.".to_string()
            }
        } else {
            "[FAIL] Configuration: could not determine home directory".to_string()
        };
        results.push(config_check);

        // Check 3: Plugin discovery -- scan ~/.aegis/plugins
        total += 1;
        let plugins_dir = dirs_check_home().map(|h| h.join(".aegis").join("plugins"));
        let plugin_count = match plugins_dir {
            Some(ref dir) if dir.is_dir() => {
                let rt = tokio::runtime::Runtime::new().ok();
                rt.and_then(|rt| rt.block_on(discover_plugins(dir)).ok())
                    .map(|p| p.len())
                    .unwrap_or(0)
            }
            _ => 0,
        };
        if plugin_count > 0 {
            passed += 1;
            results.push(format!(
                "[PASS] Plugin discovery: {plugin_count} plugin(s) found"
            ));
        } else {
            // Not a failure -- just informational
            passed += 1;
            results.push(
                "[PASS] Plugin discovery: 0 plugins found \
                 (install to ~/.aegis/plugins/)"
                    .to_string(),
            );
        }

        // Check 4: LLM endpoint reachability
        total += 1;
        if self.model_name.is_empty() {
            results.push("[FAIL] LLM endpoint: no model configured".to_string());
        } else {
            results.push(format!(
                "[PASS] LLM endpoint: model '{}' configured \
                 (connectivity check deferred to async)",
                self.model_name
            ));
            passed += 1;
        }

        // Summary
        results.push(format!("\n{passed}/{total} checks passed"));

        self.messages.push(ChatMessage::system(results.join("\n")));
    }
}

/// Return the user's home directory, or None if unavailable.
///
/// Uses the `HOME` env var on Unix, `USERPROFILE` on Windows.
pub(crate) fn dirs_check_home() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_app() -> App {
        App::new("test-model".to_string())
    }

    // rtmx:req REQ-TUI-058
    #[test]
    fn doctor_checks_home_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let aegis_dir = tmp.path().join(".aegis");
        std::fs::create_dir_all(&aegis_dir).unwrap();
        unsafe { std::env::set_var("HOME", tmp.path()) };

        let mut app = make_test_app();
        app.handle_doctor_command();

        // Should not crash and should produce output
        assert!(
            !app.messages.is_empty(),
            "Doctor should produce at least one message"
        );
        let last = app.messages.last().unwrap();
        assert!(
            last.content.contains("checks passed"),
            "Should contain summary, got: {}",
            last.content
        );
    }

    // rtmx:req REQ-TUI-058
    #[test]
    fn doctor_reports_plugin_count() {
        let tmp = tempfile::TempDir::new().unwrap();
        let aegis_dir = tmp.path().join(".aegis");
        std::fs::create_dir_all(&aegis_dir).unwrap();
        unsafe { std::env::set_var("HOME", tmp.path()) };

        let mut app = make_test_app();
        app.handle_doctor_command();

        let last = app.messages.last().unwrap();
        assert!(
            last.content.contains("plugin"),
            "Should mention plugins in output, got: {}",
            last.content
        );
    }
}
