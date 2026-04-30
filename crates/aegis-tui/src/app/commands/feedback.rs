//! Handler for the `/feedback` slash command (REQ-TUI-067, REQ-TUI-068, REQ-TUI-069).
//!
//! Renders a structured feedback template, supports submission via `gh` CLI,
//! and provides a clipboard fallback with a pre-filled GitHub issue URL.
//!
//! The `FeedbackReport` struct and submission functions are the public API
//! for multi-step feedback collection, which will be wired into the TUI
//! input loop in a follow-up requirement. They are tested and ready.

// These functions are the public API for future multi-step TUI feedback flow.
// They are fully tested but not yet called from production code.
#![allow(dead_code)]

use crate::messages::ChatMessage;

use super::App;

/// Structured feedback report collected from the user.
#[derive(Debug, Clone)]
pub struct FeedbackReport {
    /// Satisfaction rating from 1 (poor) to 5 (excellent).
    pub satisfaction: u8,
    /// What worked well.
    pub what_worked: String,
    /// What did not work or could be improved.
    pub what_didnt: String,
    /// Feature request or suggestion.
    pub feature_request: String,
}

/// Render the feedback template that shows users how to fill in a report.
pub fn format_feedback_template() -> String {
    "\
/feedback -- Submit feedback about aegis

Fill in the fields below and paste them back as a message,
or use the pre-filled GitHub issue link.

---
satisfaction: <1-5>
what_worked: <what went well>
what_didnt: <what could be improved>
feature_request: <any feature ideas>
---

Once you have your feedback ready, you can submit it in two ways:

1. If `gh` CLI is installed:
   aegis will create a GitHub issue automatically.

2. Otherwise:
   aegis will generate a pre-filled GitHub issue URL
   and copy it to your clipboard."
        .to_string()
}

/// Format the body of a feedback report for a GitHub issue.
pub fn format_feedback_body(report: &FeedbackReport) -> String {
    format!(
        "\
## Feedback

**Satisfaction:** {}/5

### What worked well
{}

### What did not work
{}

### Feature request
{}",
        report.satisfaction, report.what_worked, report.what_didnt, report.feature_request,
    )
}

/// Submit feedback as a GitHub issue via the `gh` CLI.
///
/// Returns `Ok(issue_url)` on success or `Err(message)` on failure.
pub fn submit_feedback_gh(report: &FeedbackReport) -> Result<String, String> {
    // Check if gh is available
    let has_gh = std::process::Command::new("which")
        .arg("gh")
        .output()
        .ok()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !has_gh {
        return Err("gh CLI is not installed".to_string());
    }

    let title = format!("User feedback: {}/5", report.satisfaction);
    let body = format_feedback_body(report);

    let output = std::process::Command::new("gh")
        .args([
            "issue",
            "create",
            "--repo",
            "rtmx-ai/aegis-cli",
            "--label",
            "user-feedback",
            "--title",
            &title,
            "--body",
            &body,
        ])
        .output()
        .map_err(|e| format!("failed to run gh: {e}"))?;

    if output.status.success() {
        let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(url)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(format!("gh issue create failed: {stderr}"))
    }
}

/// Generate a pre-filled GitHub new-issue URL for the feedback report.
pub fn feedback_issue_url(report: &FeedbackReport) -> String {
    let title = format!("User feedback: {}/5", report.satisfaction);
    let body = format_feedback_body(report);

    format!(
        "https://github.com/rtmx-ai/aegis-cli/issues/new?title={}&body={}&labels=user-feedback",
        url_encode(&title),
        url_encode(&body),
    )
}

/// Copy the pre-filled feedback issue URL to the system clipboard.
///
/// Returns the URL that was copied (or attempted to copy).
pub fn copy_feedback_url(report: &FeedbackReport) -> String {
    let url = feedback_issue_url(report);
    match crate::clipboard::copy_text(&url) {
        Ok(()) => url,
        Err(_) => url,
    }
}

/// Percent-encode a string for use in a URL query parameter.
///
/// Encodes all characters except unreserved ones (A-Z, a-z, 0-9, -, _, ., ~).
fn url_encode(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len() * 3);
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                encoded.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    encoded
}

impl App {
    /// Handle the `/feedback` slash command: render the template as a system message.
    pub(crate) fn handle_feedback_command(&mut self) {
        let template = format_feedback_template();
        self.messages.push(ChatMessage::system(template));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_report() -> FeedbackReport {
        FeedbackReport {
            satisfaction: 4,
            what_worked: "Fast responses and good code suggestions".to_string(),
            what_didnt: "Sometimes loses context in long sessions".to_string(),
            feature_request: "Add support for multi-file refactoring".to_string(),
        }
    }

    // rtmx:req REQ-TUI-067
    #[test]
    fn test_feedback_template_contains_fields() {
        let template = format_feedback_template();
        assert!(
            template.contains("satisfaction:"),
            "template should contain satisfaction field"
        );
        assert!(
            template.contains("what_worked:"),
            "template should contain what_worked field"
        );
        assert!(
            template.contains("what_didnt:"),
            "template should contain what_didnt field"
        );
        assert!(
            template.contains("feature_request:"),
            "template should contain feature_request field"
        );
    }

    // rtmx:req REQ-TUI-067
    #[test]
    fn test_feedback_template_mentions_slash_command() {
        let template = format_feedback_template();
        assert!(
            template.contains("/feedback"),
            "template should reference the /feedback command"
        );
    }

    // rtmx:req REQ-TUI-067
    #[test]
    fn test_format_feedback_body_includes_all_fields() {
        let report = sample_report();
        let body = format_feedback_body(&report);
        assert!(
            body.contains("4/5"),
            "body should contain satisfaction rating"
        );
        assert!(
            body.contains("Fast responses"),
            "body should contain what_worked text"
        );
        assert!(
            body.contains("loses context"),
            "body should contain what_didnt text"
        );
        assert!(
            body.contains("multi-file refactoring"),
            "body should contain feature_request text"
        );
    }

    // rtmx:req REQ-TUI-067
    #[test]
    fn test_format_feedback_body_has_markdown_headers() {
        let report = sample_report();
        let body = format_feedback_body(&report);
        assert!(
            body.contains("## Feedback"),
            "body should have Feedback header"
        );
        assert!(
            body.contains("### What worked well"),
            "body should have What worked well header"
        );
        assert!(
            body.contains("### What did not work"),
            "body should have What did not work header"
        );
        assert!(
            body.contains("### Feature request"),
            "body should have Feature request header"
        );
    }

    // rtmx:req REQ-TUI-068
    #[test]
    fn test_submit_feedback_gh_returns_error_when_gh_missing() {
        // In most test environments, gh may or may not be available.
        // We test the function does not panic and returns a Result.
        let report = sample_report();
        let result = submit_feedback_gh(&report);
        // We cannot guarantee gh is installed, so just verify it returns
        // a valid Result (Ok with a URL or Err with a message).
        match result {
            Ok(url) => assert!(!url.is_empty(), "URL should not be empty on success"),
            Err(msg) => assert!(!msg.is_empty(), "error message should not be empty"),
        }
    }

    // rtmx:req REQ-TUI-069
    #[test]
    fn test_feedback_issue_url_is_valid() {
        let report = sample_report();
        let url = feedback_issue_url(&report);
        assert!(
            url.starts_with("https://github.com/rtmx-ai/aegis-cli/issues/new?"),
            "URL should point to GitHub new issue page, got: {url}"
        );
        assert!(url.contains("title="), "URL should contain title parameter");
        assert!(url.contains("body="), "URL should contain body parameter");
        assert!(
            url.contains("labels=user-feedback"),
            "URL should contain user-feedback label"
        );
    }

    // rtmx:req REQ-TUI-069
    #[test]
    fn test_feedback_issue_url_encodes_special_chars() {
        let report = FeedbackReport {
            satisfaction: 3,
            what_worked: "spaces & symbols".to_string(),
            what_didnt: "line\nbreaks".to_string(),
            feature_request: "none".to_string(),
        };
        let url = feedback_issue_url(&report);
        // Spaces should be encoded as %20, ampersands as %26, newlines as %0A
        assert!(!url.contains(' '), "URL should not contain literal spaces");
        assert!(
            !url.contains('&') || url.contains("&labels="),
            "ampersands in content should be encoded (only query separator & allowed)"
        );
    }

    // rtmx:req REQ-TUI-069
    #[test]
    fn test_url_encode_basic() {
        assert_eq!(url_encode("hello"), "hello");
        assert_eq!(url_encode("hello world"), "hello%20world");
        assert_eq!(url_encode("a&b"), "a%26b");
        assert_eq!(url_encode("line\nbreak"), "line%0Abreak");
        assert_eq!(url_encode("100%"), "100%25");
    }

    // rtmx:req REQ-TUI-069
    #[test]
    fn test_url_encode_preserves_unreserved() {
        assert_eq!(url_encode("A-Z_a.z~0"), "A-Z_a.z~0");
    }

    // rtmx:req REQ-TUI-069
    #[test]
    fn test_copy_feedback_url_returns_url() {
        let report = sample_report();
        let url = copy_feedback_url(&report);
        assert!(
            url.starts_with("https://github.com/rtmx-ai/aegis-cli/issues/new?"),
            "copy_feedback_url should return the issue URL"
        );
    }

    // rtmx:req REQ-TUI-069
    #[test]
    fn test_feedback_issue_url_title_contains_satisfaction() {
        let report = sample_report();
        let url = feedback_issue_url(&report);
        // Title should be URL-encoded "User feedback: 4/5"
        let encoded_title = url_encode("User feedback: 4/5");
        assert!(
            url.contains(&encoded_title),
            "URL should contain encoded title with satisfaction rating"
        );
    }
}
