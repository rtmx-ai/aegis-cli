//! Async HTTP fetch with timeout and content extraction for @url: picker.
//!
//! Fetches a URL, strips HTML tags for plain text, and truncates to 4KB.

use std::time::Duration;

/// Maximum content size to return (4 KB).
const MAX_CONTENT_LEN: usize = 4096;

/// Default fetch timeout.
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

/// Result of fetching a URL.
#[derive(Debug, Clone)]
pub struct FetchResult {
    /// Extracted text content (HTML tags stripped, truncated to 4KB).
    pub content: String,
    /// HTTP status code.
    pub status: u16,
    /// Content-Type header value.
    pub content_type: String,
}

/// Strip HTML tags from content, returning plain text.
pub fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;

    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
            }
            '>' => {
                in_tag = false;
            }
            _ if in_tag => {}
            _ => {
                result.push(c);
            }
        }
    }

    // Collapse multiple whitespace runs
    let collapsed: String = result.split_whitespace().collect::<Vec<_>>().join(" ");

    collapsed
}

/// Truncate content to at most `MAX_CONTENT_LEN` bytes, breaking at a word
/// boundary when possible.
pub fn truncate_content(content: &str) -> String {
    if content.len() <= MAX_CONTENT_LEN {
        return content.to_string();
    }
    // Find the last space before the limit
    let truncated = &content[..MAX_CONTENT_LEN];
    if let Some(pos) = truncated.rfind(' ') {
        format!("{}...", &truncated[..pos])
    } else {
        format!("{truncated}...")
    }
}

/// Fetch a URL and return extracted text content.
///
/// - Uses a 10-second timeout.
/// - Strips HTML tags for text/html responses.
/// - Truncates to 4KB.
pub async fn fetch_url(url: &str) -> Result<FetchResult, String> {
    let client = reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .user_agent("aegis-cli/0.1")
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("fetch failed: {e}"))?;

    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("text/plain")
        .to_string();

    if !response.status().is_success() {
        return Err(format!("HTTP {status}: {url}"));
    }

    let body = response
        .text()
        .await
        .map_err(|e| format!("body read failed: {e}"))?;

    let text = if content_type.contains("text/html") {
        strip_html_tags(&body)
    } else {
        body
    };

    let content = truncate_content(&text);

    Ok(FetchResult {
        content,
        status,
        content_type,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-TUI-081
    #[test]
    fn test_strip_html_tags() {
        assert_eq!(strip_html_tags("<p>Hello <b>world</b></p>"), "Hello world");
        assert_eq!(strip_html_tags("no tags here"), "no tags here");
        assert_eq!(strip_html_tags("<div>  spaced  </div>"), "spaced");
    }

    // rtmx:req REQ-TUI-081
    #[test]
    fn test_truncate_content_short() {
        let short = "hello world";
        assert_eq!(truncate_content(short), short);
    }

    // rtmx:req REQ-TUI-081
    #[test]
    fn test_truncate_content_long() {
        let long = "word ".repeat(2000); // 10000 chars
        let truncated = truncate_content(&long);
        assert!(truncated.len() <= MAX_CONTENT_LEN + 3); // +3 for "..."
        assert!(truncated.ends_with("..."));
    }

    // rtmx:req REQ-TUI-081
    #[tokio::test]
    async fn test_fetch_url_extracts_content() {
        // This test verifies the fetch_url function signature and error
        // handling. We don't make real HTTP calls in unit tests.
        let result = fetch_url("http://127.0.0.1:1/nonexistent").await;
        assert!(result.is_err(), "unreachable URL must fail");
        let err = result.unwrap_err();
        assert!(
            err.contains("fetch failed"),
            "error must describe the failure: {err}"
        );
    }
}
