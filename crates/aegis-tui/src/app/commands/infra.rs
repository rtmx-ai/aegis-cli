//! /infra subcommand handler.

use crate::app::App;
use crate::messages::ChatMessage;
use aegis_infra::events::{CheckStatus, PluginEvent};
use aegis_infra::host::{Plugin, aggregate_health, discover_plugins, run_plugin};
use aegis_infra::outputs::{extract_outputs, format_outputs};
use std::path::PathBuf;

/// Discover plugins from the default plugins directory (~/.aegis/plugins).
///
/// Returns (plugins, dir_display) where dir_display is the path scanned.
fn discover_all_plugins() -> (Vec<Plugin>, String) {
    let plugins_dir = default_plugins_dir();
    let dir_display = plugins_dir
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "(unknown)".to_string());

    let plugins = match plugins_dir {
        Some(dir) if dir.is_dir() => {
            let rt = tokio::runtime::Runtime::new().ok();
            rt.and_then(|rt| rt.block_on(discover_plugins(&dir)).ok())
                .unwrap_or_default()
        }
        _ => Vec::new(),
    };

    (plugins, dir_display)
}

/// Return the default plugins directory: ~/.aegis/plugins.
fn default_plugins_dir() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".aegis").join("plugins"))
    }
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join(".aegis").join("plugins"))
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

impl App {
    /// Handle /infra subcommands: status, list, preview, up, destroy.
    pub(crate) fn handle_infra_command(&mut self, sub: &str) {
        let parts: Vec<&str> = sub.split_whitespace().collect();
        let subcmd = parts.first().copied().unwrap_or("");

        match subcmd {
            "list" => self.infra_list(),
            "status" => self.infra_status(),
            "preview" => {
                if parts.len() < 2 {
                    self.messages.push(ChatMessage::error(
                        "Usage: /infra preview <plugin-name>".to_string(),
                    ));
                } else {
                    self.infra_run_subcommand(parts[1], "preview", None, 60);
                }
            }
            "up" => {
                if parts.len() < 2 {
                    self.messages.push(ChatMessage::error(
                        "Usage: /infra up <plugin-name>".to_string(),
                    ));
                } else {
                    // Remaining args after plugin name become input JSON
                    let input = if parts.len() > 2 {
                        Some(parts[2..].join(" "))
                    } else {
                        None
                    };
                    self.infra_run_subcommand(parts[1], "up", input.as_deref(), 300);
                }
            }
            "destroy" => {
                if parts.len() < 2 {
                    self.messages.push(ChatMessage::error(
                        "Usage: /infra destroy <plugin-name>".to_string(),
                    ));
                } else {
                    self.infra_run_subcommand(parts[1], "destroy", None, 300);
                }
            }
            "" => {
                self.messages.push(ChatMessage::system(
                    "Usage: /infra <status|list|preview|up|destroy> [plugin-name]".to_string(),
                ));
            }
            other => {
                self.messages.push(ChatMessage::error(format!(
                    "Unknown /infra subcommand: {other}. \
                     Try: status, list, preview, up, destroy"
                )));
            }
        }
    }

    /// List all discovered plugins.
    fn infra_list(&mut self) {
        let (plugins, dir_display) = discover_all_plugins();

        if plugins.is_empty() {
            self.messages.push(ChatMessage::system(format!(
                "[infra list] No aegis-infra/v1 plugins found.\n\
                 Searched: {dir_display}\n\
                 Install plugin binaries to ~/.aegis/plugins/ to enable."
            )));
        } else {
            let mut lines = vec![format!(
                "[infra list] {} plugin(s) found in {dir_display}:",
                plugins.len()
            )];
            for p in &plugins {
                let desc = p
                    .manifest
                    .description
                    .as_deref()
                    .unwrap_or("(no description)");
                lines.push(format!(
                    "  {} v{} -- {}",
                    p.manifest.name, p.manifest.version, desc
                ));
            }
            self.messages.push(ChatMessage::system(lines.join("\n")));
        }
    }

    /// Run status checks on all discovered plugins.
    fn infra_status(&mut self) {
        let (plugins, dir_display) = discover_all_plugins();

        if plugins.is_empty() {
            self.messages.push(ChatMessage::system(format!(
                "[infra status] No plugins discovered in {dir_display}.\n\
                 Install aegis-infra/v1 plugins to ~/.aegis/plugins/ to enable."
            )));
            return;
        }

        let mut lines = vec![format!(
            "[infra status] Checking {} plugin(s)...",
            plugins.len()
        )];

        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                self.messages.push(ChatMessage::error(format!(
                    "[infra status] Failed to create async runtime: {e}"
                )));
                return;
            }
        };

        for plugin in &plugins {
            match rt.block_on(run_plugin(plugin, "status", None, 30)) {
                Ok(output) => {
                    let checks: Vec<_> = output
                        .events
                        .iter()
                        .filter_map(|e| match e {
                            PluginEvent::Check(c) => Some(c.clone()),
                            _ => None,
                        })
                        .collect();

                    if checks.is_empty() {
                        lines.push(format!(
                            "  {}: no health checks reported",
                            plugin.manifest.name
                        ));
                    } else {
                        let (healthy, summary) = aggregate_health(&checks);
                        let indicator = if healthy { "HEALTHY" } else { "UNHEALTHY" };
                        lines.push(format!(
                            "  {} [{}]: {}",
                            plugin.manifest.name, indicator, summary
                        ));

                        // Show details for failed/warned checks
                        for c in &checks {
                            if c.status != CheckStatus::Pass {
                                let detail = c.detail.as_deref().unwrap_or("(no detail)");
                                lines.push(format!("    {:?} {}: {}", c.status, c.name, detail));
                            }
                        }
                    }
                }
                Err(e) => {
                    lines.push(format!("  {} [ERROR]: {}", plugin.manifest.name, e));
                }
            }
        }

        self.messages.push(ChatMessage::system(lines.join("\n")));
    }

    /// Run a subcommand (preview, up, destroy) on a named plugin.
    fn infra_run_subcommand(
        &mut self,
        plugin_name: &str,
        subcommand: &str,
        input_json: Option<&str>,
        timeout_secs: u64,
    ) {
        let (plugins, _dir_display) = discover_all_plugins();

        let plugin = plugins.iter().find(|p| p.manifest.name == plugin_name);

        let plugin = match plugin {
            Some(p) => p.clone(),
            None => {
                self.messages.push(ChatMessage::error(format!(
                    "[infra {subcommand}] Plugin '{plugin_name}' not found. \
                     Run /infra list to see available plugins."
                )));
                return;
            }
        };

        self.messages.push(ChatMessage::system(format!(
            "[infra {subcommand}] Running '{subcommand}' on plugin \
             '{plugin_name}'..."
        )));

        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                self.messages.push(ChatMessage::error(format!(
                    "[infra {subcommand}] Failed to create async runtime: {e}"
                )));
                return;
            }
        };

        match rt.block_on(run_plugin(&plugin, subcommand, input_json, timeout_secs)) {
            Ok(output) => {
                let mut lines = Vec::new();

                // Show progress and diagnostic events
                for event in &output.events {
                    match event {
                        PluginEvent::Progress(p) => {
                            let name = p.name.as_deref().unwrap_or(&p.resource);
                            lines.push(format!(
                                "  [{}] {} {} -- {}",
                                p.status, p.operation, name, p.resource
                            ));
                        }
                        PluginEvent::Diagnostic(d) => {
                            lines.push(format!("  [{}] {}", d.severity, d.message));
                        }
                        PluginEvent::Check(c) => {
                            let detail = c.detail.as_deref().unwrap_or("");
                            lines.push(format!("  [{:?}] {}: {}", c.status, c.name, detail));
                        }
                        PluginEvent::Result(_) => {
                            // Handled separately below
                        }
                    }
                }

                // Show final result
                if let Some(ref result) = output.result {
                    if result.success {
                        let summary = result
                            .summary
                            .as_deref()
                            .unwrap_or("completed successfully");
                        lines.push(format!("\n[infra {subcommand}] {plugin_name}: {summary}"));

                        // Show outputs for 'up' subcommand
                        if subcommand == "up" {
                            let outputs = extract_outputs(&output);
                            let formatted = format_outputs(&outputs);
                            if !formatted.is_empty() {
                                lines.push("Outputs:".to_string());
                                lines.push(formatted);
                            }
                        }
                    } else {
                        let err_msg = result.error.as_deref().unwrap_or("unknown error");
                        lines.push(format!(
                            "\n[infra {subcommand}] {plugin_name} FAILED: \
                             {err_msg}"
                        ));
                    }
                }

                self.messages.push(ChatMessage::system(lines.join("\n")));
            }
            Err(e) => {
                self.messages.push(ChatMessage::error(format!(
                    "[infra {subcommand}] {plugin_name}: {e}"
                )));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_app() -> App {
        App::new("test-model".to_string())
    }

    // rtmx:req REQ-INFRA-011
    #[test]
    fn infra_list_with_no_plugins() {
        let mut app = make_test_app();
        // Set HOME to a temp dir with no plugins subdir
        let tmp = tempfile::TempDir::new().unwrap();
        unsafe { std::env::set_var("HOME", tmp.path()) };

        app.handle_infra_command("list");

        let last = app.messages.last().unwrap();
        assert!(
            last.content.contains("No aegis-infra/v1 plugins found"),
            "Expected no-plugins message, got: {}",
            last.content
        );
    }

    // rtmx:req REQ-INFRA-011
    #[test]
    fn infra_status_with_no_plugins() {
        let mut app = make_test_app();
        let tmp = tempfile::TempDir::new().unwrap();
        unsafe { std::env::set_var("HOME", tmp.path()) };

        app.handle_infra_command("status");

        let last = app.messages.last().unwrap();
        assert!(
            last.content.contains("No plugins discovered"),
            "Expected no-plugins status, got: {}",
            last.content
        );
    }

    // rtmx:req REQ-INFRA-011
    #[test]
    fn infra_preview_requires_plugin_name() {
        let mut app = make_test_app();
        app.handle_infra_command("preview");

        let last = app.messages.last().unwrap();
        assert!(
            last.content.contains("Usage: /infra preview <plugin-name>"),
            "Expected usage message, got: {}",
            last.content
        );
    }

    // rtmx:req REQ-INFRA-011
    #[test]
    fn infra_up_requires_plugin_name() {
        let mut app = make_test_app();
        app.handle_infra_command("up");

        let last = app.messages.last().unwrap();
        assert!(
            last.content.contains("Usage: /infra up <plugin-name>"),
            "Expected usage message, got: {}",
            last.content
        );
    }

    // rtmx:req REQ-INFRA-011
    #[test]
    fn infra_destroy_requires_plugin_name() {
        let mut app = make_test_app();
        app.handle_infra_command("destroy");

        let last = app.messages.last().unwrap();
        assert!(
            last.content.contains("Usage: /infra destroy <plugin-name>"),
            "Expected usage message, got: {}",
            last.content
        );
    }
}
