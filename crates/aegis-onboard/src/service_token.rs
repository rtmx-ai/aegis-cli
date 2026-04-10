//! Enterprise service token authentication for BYOC mode.
//!
//! When Enterprise BYOC mode is detected, supports service token auth:
//! 1. Check `AEGIS_SERVICE_TOKEN` env var
//! 2. Check token file at `~/.aegis/token` or a config-specified path
//! 3. Token is sent as `Authorization: Bearer <token>` header

use std::path::Path;

/// Service token authentication for enterprise BYOC mode.
///
/// The token is loaded from environment or file and used to
/// authenticate requests to the enterprise gateway. The token
/// value is held in memory only -- never written to config.yaml.
#[derive(Debug, Clone)]
pub struct ServiceTokenAuth {
    token: String,
}

impl ServiceTokenAuth {
    /// Attempt to load a service token from environment or file.
    ///
    /// Resolution order:
    /// 1. `AEGIS_SERVICE_TOKEN` environment variable
    /// 2. Token file at `<config_dir>/token` (e.g., `~/.aegis/token`)
    ///
    /// Returns `None` if no token source is found.
    pub fn from_env_or_file(config_dir: &Path) -> Option<Self> {
        // 1. Environment variable
        if let Some(token) = non_empty_env("AEGIS_SERVICE_TOKEN") {
            return Some(Self { token });
        }

        // 2. Token file
        let token_path = config_dir.join("token");
        if let Some(token) = read_token_file(&token_path) {
            return Some(Self { token });
        }

        None
    }

    /// Attempt to load from env, file at config_dir, or a custom path.
    ///
    /// Resolution order:
    /// 1. `AEGIS_SERVICE_TOKEN` environment variable
    /// 2. Custom token path (if provided)
    /// 3. Token file at `<config_dir>/token`
    pub fn from_env_or_paths(config_dir: &Path, custom_path: Option<&Path>) -> Option<Self> {
        // 1. Environment variable
        if let Some(token) = non_empty_env("AEGIS_SERVICE_TOKEN") {
            return Some(Self { token });
        }

        // 2. Custom path
        if let Some(path) = custom_path
            && let Some(token) = read_token_file(path)
        {
            return Some(Self { token });
        }

        // 3. Default token file
        let token_path = config_dir.join("token");
        if let Some(token) = read_token_file(&token_path) {
            return Some(Self { token });
        }

        None
    }

    /// Build the Authorization header value for HTTP requests.
    pub fn authorization_header(&self) -> String {
        format!("Bearer {}", self.token)
    }

    /// Return a reference to the raw token value.
    pub fn token(&self) -> &str {
        &self.token
    }
}

/// Read a token from a file, trimming whitespace.
///
/// Returns `None` if the file does not exist, is unreadable,
/// or contains only whitespace.
fn read_token_file(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

/// Return the token source description for config metadata.
///
/// This records WHERE the token came from (env var or file path)
/// without storing the actual token value.
pub fn token_source_label(config_dir: &Path) -> Option<String> {
    if non_empty_env("AEGIS_SERVICE_TOKEN").is_some() {
        return Some("env:AEGIS_SERVICE_TOKEN".to_string());
    }
    let token_path = config_dir.join("token");
    if token_path.exists() {
        return Some(format!("file:{}", token_path.display()));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // @req REQ-ONBOARD-019
    //
    // All service token scenarios run in a single test to avoid
    // race conditions from parallel tests mutating process-wide
    // environment variables (AEGIS_SERVICE_TOKEN).
    #[test]
    fn service_token_scenarios() {
        fn clear() {
            unsafe { std::env::remove_var("AEGIS_SERVICE_TOKEN") };
        }

        // -- from env var --
        clear();
        unsafe { std::env::set_var("AEGIS_SERVICE_TOKEN", "tok-abc-123") };
        let tmp = TempDir::new().unwrap();
        let auth = ServiceTokenAuth::from_env_or_file(tmp.path());
        assert!(auth.is_some());
        assert_eq!(auth.unwrap().token(), "tok-abc-123");

        // -- from token file (env cleared) --
        clear();
        let tmp2 = TempDir::new().unwrap();
        std::fs::write(tmp2.path().join("token"), "file-token-xyz\n").unwrap();
        let auth = ServiceTokenAuth::from_env_or_file(tmp2.path());
        assert!(auth.is_some());
        assert_eq!(auth.unwrap().token(), "file-token-xyz");

        // -- env var takes priority over file --
        let tmp3 = TempDir::new().unwrap();
        std::fs::write(tmp3.path().join("token"), "file-token").unwrap();
        unsafe { std::env::set_var("AEGIS_SERVICE_TOKEN", "env-token") };
        let auth = ServiceTokenAuth::from_env_or_file(tmp3.path());
        assert_eq!(
            auth.unwrap().token(),
            "env-token",
            "Env var should take priority"
        );

        // -- returns None when no token source --
        clear();
        let tmp4 = TempDir::new().unwrap();
        let auth = ServiceTokenAuth::from_env_or_file(tmp4.path());
        assert!(auth.is_none(), "Should be None when no token source");

        // -- authorization header format --
        unsafe { std::env::set_var("AEGIS_SERVICE_TOKEN", "my-secret-token") };
        let tmp5 = TempDir::new().unwrap();
        let auth = ServiceTokenAuth::from_env_or_file(tmp5.path()).unwrap();
        assert_eq!(auth.authorization_header(), "Bearer my-secret-token");

        // -- ignores empty env var --
        clear();
        unsafe { std::env::set_var("AEGIS_SERVICE_TOKEN", "") };
        let tmp6 = TempDir::new().unwrap();
        let auth = ServiceTokenAuth::from_env_or_file(tmp6.path());
        assert!(auth.is_none(), "Empty env var should be ignored");

        // -- ignores whitespace-only token file --
        clear();
        let tmp_ws = TempDir::new().unwrap();
        std::fs::write(tmp_ws.path().join("token"), "   \n  \t  \n").unwrap();
        let auth = ServiceTokenAuth::from_env_or_file(tmp_ws.path());
        assert!(
            auth.is_none(),
            "Whitespace-only token file should be ignored"
        );

        // -- from custom path --
        clear();
        let tmp_cp = TempDir::new().unwrap();
        let custom = tmp_cp.path().join("custom-token");
        std::fs::write(&custom, "custom-tok-999").unwrap();
        let auth = ServiceTokenAuth::from_env_or_paths(tmp_cp.path(), Some(custom.as_path()));
        assert_eq!(auth.unwrap().token(), "custom-tok-999");

        // -- token source label from env --
        clear();
        unsafe { std::env::set_var("AEGIS_SERVICE_TOKEN", "some-token") };
        let tmp7 = TempDir::new().unwrap();
        let label = token_source_label(tmp7.path());
        assert_eq!(label, Some("env:AEGIS_SERVICE_TOKEN".to_string()));

        // -- token source label from file --
        clear();
        let tmp_lf = TempDir::new().unwrap();
        std::fs::write(tmp_lf.path().join("token"), "file-tok").unwrap();
        let label = token_source_label(tmp_lf.path());
        assert!(label.is_some());
        assert!(
            label.unwrap().starts_with("file:"),
            "Label should indicate file source"
        );

        // -- token source label none when no source --
        clear();
        let tmp8 = TempDir::new().unwrap();
        let label = token_source_label(tmp8.path());
        assert!(label.is_none());

        // Final cleanup
        clear();
    }
}
