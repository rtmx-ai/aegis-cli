//! Plugin auto-download: detect, download, and install plugin binaries from
//! GitHub releases (REQ-ONBOARD-024).
//!
//! When `aegis init` detects cloud mode and the required plugin is not on PATH
//! or in `~/.aegis/plugins/`, this module handles fetching it from the latest
//! GitHub release.

use std::path::{Path, PathBuf};

/// Configuration for downloading a plugin from GitHub releases.
#[derive(Debug, Clone)]
pub struct PluginDownloadConfig {
    /// GitHub repository in `owner/repo` format.
    pub repo: String,
    /// Name of the plugin binary.
    pub binary_name: String,
    /// Local directory where the plugin should be installed.
    pub install_dir: PathBuf,
}

/// Errors that can occur during plugin download.
#[derive(Debug)]
pub enum DownloadError {
    /// Network or HTTP error.
    Network(String),
    /// The release does not contain a binary for this platform.
    NoPlatformBinary(String),
    /// Filesystem error writing the binary.
    Io(std::io::Error),
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadError::Network(msg) => write!(f, "network error: {msg}"),
            DownloadError::NoPlatformBinary(msg) => {
                write!(f, "no binary for platform: {msg}")
            }
            DownloadError::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for DownloadError {}

impl From<std::io::Error> for DownloadError {
    fn from(e: std::io::Error) -> Self {
        DownloadError::Io(e)
    }
}

/// Check whether the plugin binary already exists in the install directory.
pub fn plugin_installed(config: &PluginDownloadConfig) -> bool {
    config.install_dir.join(&config.binary_name).exists()
}

/// Return the full path where the plugin binary would be installed.
pub fn plugin_binary_path(config: &PluginDownloadConfig) -> PathBuf {
    config.install_dir.join(&config.binary_name)
}

/// Build the default plugin download config for gcp-assured-workloads.
pub fn default_plugin_config(home_dir: &Path) -> PluginDownloadConfig {
    PluginDownloadConfig {
        repo: "rtmx-ai/gcp-assured-workloads".into(),
        binary_name: "gcp-assured-workloads".into(),
        install_dir: home_dir.join(".aegis/plugins"),
    }
}

/// Return the GitHub API URL for the latest release of the configured repo.
pub fn latest_release_url(config: &PluginDownloadConfig) -> String {
    format!(
        "https://api.github.com/repos/{}/releases/latest",
        config.repo
    )
}

/// Detect the platform suffix for GitHub release asset names.
///
/// Returns a string like `linux-x86_64`, `darwin-arm64`, `darwin-x86_64`,
/// or `windows-x86_64`.
pub fn platform_asset_suffix() -> &'static str {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "linux-x86_64"
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "darwin-arm64"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "darwin-x86_64"
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "windows-x86_64"
    }
}

/// Download the plugin binary from GitHub releases.
///
/// In the real implementation this will:
/// 1. Query the GitHub releases API for the latest release
/// 2. Find the asset matching the current platform
/// 3. Download and extract the binary
/// 4. Place it in `config.install_dir` and make it executable
///
/// Currently returns the expected install path; actual HTTP download is
/// deferred to integration testing with a real network.
pub async fn download_plugin(config: &PluginDownloadConfig) -> Result<PathBuf, DownloadError> {
    // Ensure the install directory exists
    std::fs::create_dir_all(&config.install_dir)?;

    let dest = config.install_dir.join(&config.binary_name);
    // TODO: Implement actual HTTP download from GitHub releases API.
    // For now, return the expected destination path.
    // The actual download will use reqwest to:
    //   GET {latest_release_url} -> parse JSON -> find asset for
    //   {platform_asset_suffix} -> download -> extract -> chmod +x
    Ok(dest)
}

/// Ensure the plugin directory exists, creating it if needed.
pub fn ensure_plugin_dir(config: &PluginDownloadConfig) -> std::io::Result<()> {
    std::fs::create_dir_all(&config.install_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // @req REQ-ONBOARD-024
    #[test]
    fn test_plugin_installed_returns_false_when_missing() {
        let tmp = TempDir::new().unwrap();
        let config = PluginDownloadConfig {
            repo: "rtmx-ai/gcp-assured-workloads".into(),
            binary_name: "gcp-assured-workloads".into(),
            install_dir: tmp.path().join("plugins"),
        };
        assert!(
            !plugin_installed(&config),
            "plugin_installed must return false when binary does not exist"
        );
    }

    // @req REQ-ONBOARD-024
    #[test]
    fn test_plugin_installed_returns_true_when_present() {
        let tmp = TempDir::new().unwrap();
        let plugins_dir = tmp.path().join("plugins");
        std::fs::create_dir_all(&plugins_dir).unwrap();
        let binary = plugins_dir.join("gcp-assured-workloads");
        std::fs::write(&binary, b"#!/bin/sh\necho mock").unwrap();

        let config = PluginDownloadConfig {
            repo: "rtmx-ai/gcp-assured-workloads".into(),
            binary_name: "gcp-assured-workloads".into(),
            install_dir: plugins_dir,
        };
        assert!(
            plugin_installed(&config),
            "plugin_installed must return true when binary exists"
        );
    }

    // @req REQ-ONBOARD-024
    #[test]
    fn test_default_plugin_config_paths() {
        let home = std::path::Path::new("/home/testuser");
        let config = default_plugin_config(home);
        assert_eq!(config.repo, "rtmx-ai/gcp-assured-workloads");
        assert_eq!(config.binary_name, "gcp-assured-workloads");
        assert_eq!(
            config.install_dir,
            PathBuf::from("/home/testuser/.aegis/plugins")
        );
    }

    // @req REQ-ONBOARD-024
    #[test]
    fn test_plugin_binary_path() {
        let config = PluginDownloadConfig {
            repo: "rtmx-ai/gcp-assured-workloads".into(),
            binary_name: "gcp-assured-workloads".into(),
            install_dir: PathBuf::from("/home/user/.aegis/plugins"),
        };
        assert_eq!(
            plugin_binary_path(&config),
            PathBuf::from("/home/user/.aegis/plugins/gcp-assured-workloads")
        );
    }

    // @req REQ-ONBOARD-024
    #[test]
    fn test_latest_release_url() {
        let config = PluginDownloadConfig {
            repo: "rtmx-ai/gcp-assured-workloads".into(),
            binary_name: "gcp-assured-workloads".into(),
            install_dir: PathBuf::from("/tmp"),
        };
        assert_eq!(
            latest_release_url(&config),
            "https://api.github.com/repos/rtmx-ai/gcp-assured-workloads/releases/latest"
        );
    }

    // @req REQ-ONBOARD-024
    #[test]
    fn test_platform_asset_suffix_is_nonempty() {
        let suffix = platform_asset_suffix();
        assert!(
            !suffix.is_empty(),
            "platform_asset_suffix must return a non-empty string"
        );
        // Must contain a known OS prefix
        assert!(
            suffix.starts_with("linux")
                || suffix.starts_with("darwin")
                || suffix.starts_with("windows"),
            "platform suffix must start with a known OS: {suffix}"
        );
    }

    // @req REQ-ONBOARD-024
    #[tokio::test]
    async fn test_download_plugin_creates_install_dir() {
        let tmp = TempDir::new().unwrap();
        let plugins_dir = tmp.path().join("nested/plugins");
        let config = PluginDownloadConfig {
            repo: "rtmx-ai/gcp-assured-workloads".into(),
            binary_name: "gcp-assured-workloads".into(),
            install_dir: plugins_dir.clone(),
        };
        let result = download_plugin(&config).await;
        assert!(result.is_ok(), "download_plugin should succeed");
        assert!(
            plugins_dir.exists(),
            "download_plugin must create install_dir"
        );
        let dest = result.unwrap();
        assert_eq!(
            dest,
            plugins_dir.join("gcp-assured-workloads"),
            "returned path must be the expected binary location"
        );
    }

    // @req REQ-ONBOARD-024
    #[test]
    fn test_ensure_plugin_dir_creates_directory() {
        let tmp = TempDir::new().unwrap();
        let plugins_dir = tmp.path().join("deep/nested/plugins");
        let config = PluginDownloadConfig {
            repo: "rtmx-ai/gcp-assured-workloads".into(),
            binary_name: "gcp-assured-workloads".into(),
            install_dir: plugins_dir.clone(),
        };
        ensure_plugin_dir(&config).unwrap();
        assert!(plugins_dir.exists());
        assert!(plugins_dir.is_dir());
    }

    // @req REQ-ONBOARD-024
    #[test]
    fn test_download_error_display() {
        let err = DownloadError::Network("timeout".into());
        assert_eq!(format!("{err}"), "network error: timeout");

        let err = DownloadError::NoPlatformBinary("arm32".into());
        assert_eq!(format!("{err}"), "no binary for platform: arm32");

        let err = DownloadError::Io(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "cannot write",
        ));
        assert!(format!("{err}").contains("I/O error"));
    }
}
