//! Post-initialization connectivity verification.
//!
//! After aegis init, verifies the configured LLM endpoint is
//! reachable. Reports latency on success, actionable error on failure.

use std::time::{Duration, Instant};

/// Result of a connectivity check.
#[derive(Debug)]
pub struct ConnectivityResult {
    pub reachable: bool,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
}

/// Check connectivity to an LLM endpoint.
///
/// Sends a minimal HTTP request and measures round-trip time.
/// For local endpoints, connects to the health/version endpoint.
/// Does NOT send any LLM payload.
pub async fn check_endpoint(endpoint: &str, timeout: Duration) -> ConnectivityResult {
    let client = match reqwest::Client::builder().connect_timeout(timeout).build() {
        Ok(c) => c,
        Err(e) => {
            return ConnectivityResult {
                reachable: false,
                latency_ms: None,
                error: Some(format!("Failed to create HTTP client: {e}")),
            };
        }
    };

    let start = Instant::now();
    let url = format!("{endpoint}/models");

    match client.get(&url).timeout(timeout).send().await {
        Ok(resp) => {
            let latency = start.elapsed().as_millis() as u64;
            // Any response (even 401/404) means the endpoint is reachable
            ConnectivityResult {
                reachable: true,
                latency_ms: Some(latency),
                error: if !resp.status().is_success()
                    && resp.status().as_u16() != 401
                    && resp.status().as_u16() != 404
                {
                    Some(format!("Endpoint returned {}", resp.status()))
                } else {
                    None
                },
            }
        }
        Err(e) => ConnectivityResult {
            reachable: false,
            latency_ms: Some(start.elapsed().as_millis() as u64),
            error: Some(format!("Cannot reach {endpoint}: {e}")),
        },
    }
}

/// Validate connectivity and return a human-readable summary.
pub fn format_result(result: &ConnectivityResult) -> String {
    if result.reachable {
        if let Some(ms) = result.latency_ms {
            format!("Endpoint reachable ({ms}ms)")
        } else {
            "Endpoint reachable".to_string()
        }
    } else {
        format!(
            "Endpoint unreachable: {}",
            result.error.as_deref().unwrap_or("unknown error")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-ONBOARD-012
    #[test]
    fn format_reachable_with_latency() {
        let result = ConnectivityResult {
            reachable: true,
            latency_ms: Some(42),
            error: None,
        };
        let msg = format_result(&result);
        assert!(msg.contains("reachable"));
        assert!(msg.contains("42ms"));
    }

    // rtmx:req REQ-ONBOARD-012
    #[test]
    fn format_unreachable_with_error() {
        let result = ConnectivityResult {
            reachable: false,
            latency_ms: None,
            error: Some("connection refused".to_string()),
        };
        let msg = format_result(&result);
        assert!(msg.contains("unreachable"));
        assert!(msg.contains("connection refused"));
    }

    // rtmx:req REQ-ONBOARD-012
    #[test]
    fn format_reachable_without_latency() {
        let result = ConnectivityResult {
            reachable: true,
            latency_ms: None,
            error: None,
        };
        let msg = format_result(&result);
        assert!(msg.contains("reachable"));
        assert!(!msg.contains("ms"));
    }

    // rtmx:req REQ-ONBOARD-012
    #[tokio::test]
    async fn unreachable_endpoint_returns_false() {
        let result = check_endpoint("http://127.0.0.1:1", Duration::from_secs(1)).await;
        assert!(!result.reachable);
        assert!(result.error.is_some());
    }

    // rtmx:req REQ-ONBOARD-012
    #[test]
    fn format_unreachable_without_error() {
        let result = ConnectivityResult {
            reachable: false,
            latency_ms: None,
            error: None,
        };
        let msg = format_result(&result);
        assert!(msg.contains("unknown error"));
    }
}
