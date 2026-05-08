//! OSC 8 terminal hyperlinks for clickable URLs in TUI output.
//!
//! OSC 8 is supported by iTerm2, tmux 3.4+, Windows Terminal, and
//! GNOME Terminal. Terminals that do not support OSC 8 display the
//! link text as plain text with the URL visible, so this is a
//! progressive enhancement.
//!
//! Format: `\x1b]8;;<url>\x07<link text>\x1b]8;;\x07`

use ratatui::style::{Color, Style};
use ratatui::text::Span;

/// OSC 8 escape sequence to open a hyperlink.
const OSC8_OPEN: &str = "\x1b]8;;";
/// String Terminator for OSC sequences.
const OSC8_ST: &str = "\x07";
/// OSC 8 escape sequence to close a hyperlink.
const OSC8_CLOSE: &str = "\x1b]8;;\x07";

/// Wrap a URL in OSC 8 escape sequences so terminals render it as a
/// clickable hyperlink. The visible text is the URL itself.
pub fn osc8_wrap(url: &str) -> String {
    format!("{OSC8_OPEN}{url}{OSC8_ST}{url}{OSC8_CLOSE}")
}

/// Parse text and split it into spans, wrapping any `https://` URLs
/// with OSC 8 hyperlink escape sequences. Plain text segments become
/// default-styled spans; URLs become cyan underlined spans with OSC 8.
///
/// Returns a `Vec<Span>` suitable for inclusion in a ratatui `Line`.
pub fn render_with_hyperlinks(text: &str) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut remaining = text;

    while let Some(start) = remaining.find("https://") {
        // Push any text before the URL
        if start > 0 {
            spans.push(Span::raw(remaining[..start].to_string()));
        }

        // Find the end of the URL (first whitespace or end of string)
        let url_start = &remaining[start..];
        let end = url_start
            .find(|c: char| c.is_whitespace())
            .unwrap_or(url_start.len());
        let url = &url_start[..end];

        // Build the OSC 8-wrapped span with visual styling
        let wrapped = osc8_wrap(url);
        spans.push(Span::styled(
            wrapped,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(ratatui::style::Modifier::UNDERLINED),
        ));

        remaining = &url_start[end..];
    }

    // Push any trailing text
    if !remaining.is_empty() {
        spans.push(Span::raw(remaining.to_string()));
    }

    spans
}

/// Check if a string looks like a URL (starts with `http://` or `https://`).
pub fn is_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

/// Detect a URL in text that follows the `@url:` prefix.
///
/// Returns the URL substring if found. Leading/trailing whitespace around
/// the URL portion is trimmed. Returns `None` when the input does not start
/// with `@url:` or the value after the prefix is not a URL.
pub fn detect_url_in_at_trigger(input: &str) -> Option<&str> {
    let trimmed = input.trim();
    let url_part = trimmed.strip_prefix("@url:")?.trim();
    if is_url(url_part) {
        Some(url_part)
    } else {
        None
    }
}

/// Enrich an auth guidance message with relevant documentation URLs
/// based on the cloud provider.
pub fn enrich_auth_guidance(
    base_message: &str,
    provider: &crate::app::ConnectProvider,
) -> String {
    use crate::app::ConnectProvider;
    match provider {
        ConnectProvider::Vertex => {
            format!(
                "{base_message}\n\
                 Install: https://cloud.google.com/sdk/docs/install\n\
                 Auth: https://accounts.google.com/"
            )
        }
        ConnectProvider::Bedrock => {
            format!(
                "{base_message}\n\
                 Docs: https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html"
            )
        }
        ConnectProvider::Azure => {
            format!(
                "{base_message}\n\
                 Install: https://learn.microsoft.com/en-us/cli/azure/install-azure-cli"
            )
        }
        ConnectProvider::Local => base_message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-TUI-073
    #[test]
    fn test_osc8_hyperlink_wraps_url() {
        let spans =
            render_with_hyperlinks("Visit https://cloud.google.com/sdk/docs/install for details");
        // Should have 3 spans: prefix text, wrapped URL, suffix text
        assert_eq!(
            spans.len(),
            3,
            "expected 3 spans (prefix, URL, suffix): {spans:?}"
        );

        // The URL span should contain OSC 8 sequences
        let url_span = &spans[1];
        let content = url_span.content.as_ref();
        assert!(
            content.contains("\x1b]8;;"),
            "URL span should contain OSC 8 open sequence: {content:?}"
        );
        assert!(
            content.contains("https://cloud.google.com/sdk/docs/install"),
            "URL span should contain the URL: {content:?}"
        );
        assert!(
            content.contains("\x1b]8;;\x07"),
            "URL span should contain OSC 8 close sequence: {content:?}"
        );

        // The URL span should be styled cyan + underlined
        assert_eq!(url_span.style.fg, Some(Color::Cyan), "URL should be cyan");
        assert!(
            url_span
                .style
                .add_modifier
                .contains(ratatui::style::Modifier::UNDERLINED),
            "URL should be underlined"
        );
    }

    // rtmx:req REQ-TUI-073
    #[test]
    fn test_plain_text_has_no_hyperlinks() {
        let spans = render_with_hyperlinks("No URLs here, just plain text.");
        assert_eq!(spans.len(), 1, "plain text should produce one span");
        assert_eq!(spans[0].content.as_ref(), "No URLs here, just plain text.");
        // Should not contain any OSC 8 sequences
        assert!(
            !spans[0].content.contains("\x1b]8;;"),
            "plain text should have no OSC 8 sequences"
        );
    }

    // rtmx:req REQ-TUI-073
    #[test]
    fn test_multiple_urls_all_wrapped() {
        let text = "See https://example.com/a and https://example.com/b for info";
        let spans = render_with_hyperlinks(text);
        // Expected: "See ", url1, " and ", url2, " for info"
        assert_eq!(
            spans.len(),
            5,
            "two URLs with surrounding text should produce 5 spans: {spans:?}"
        );

        // Both URL spans should contain OSC 8 sequences
        let url1 = &spans[1];
        assert!(
            url1.content.contains("https://example.com/a"),
            "first URL should be wrapped: {:?}",
            url1.content
        );
        assert!(url1.content.contains("\x1b]8;;"));

        let url2 = &spans[3];
        assert!(
            url2.content.contains("https://example.com/b"),
            "second URL should be wrapped: {:?}",
            url2.content
        );
        assert!(url2.content.contains("\x1b]8;;"));
    }

    // rtmx:req REQ-TUI-073
    #[test]
    fn test_auth_guidance_contains_hyperlink() {
        use crate::app::{ConnectProvider, auth_guidance};

        // Vertex auth guidance should contain a URL that would be wrapped
        let vertex_msg = auth_guidance(&ConnectProvider::Vertex);
        // The guidance text itself may not have URLs, but the enriched
        // version (with URLs added) should. Test the enrichment function.
        let enriched = enrich_auth_guidance(vertex_msg, &ConnectProvider::Vertex);
        let spans = render_with_hyperlinks(&enriched);
        let has_osc8 = spans.iter().any(|s| s.content.contains("\x1b]8;;"));
        assert!(
            has_osc8,
            "enriched vertex auth guidance should produce OSC 8 hyperlinks: {spans:?}"
        );

        // Bedrock guidance
        let bedrock_msg = auth_guidance(&ConnectProvider::Bedrock);
        let enriched = enrich_auth_guidance(bedrock_msg, &ConnectProvider::Bedrock);
        let spans = render_with_hyperlinks(&enriched);
        let has_osc8 = spans.iter().any(|s| s.content.contains("\x1b]8;;"));
        assert!(
            has_osc8,
            "enriched bedrock auth guidance should produce OSC 8 hyperlinks: {spans:?}"
        );

        // Azure guidance
        let azure_msg = auth_guidance(&ConnectProvider::Azure);
        let enriched = enrich_auth_guidance(azure_msg, &ConnectProvider::Azure);
        let spans = render_with_hyperlinks(&enriched);
        let has_osc8 = spans.iter().any(|s| s.content.contains("\x1b]8;;"));
        assert!(
            has_osc8,
            "enriched azure auth guidance should produce OSC 8 hyperlinks: {spans:?}"
        );
    }

    // rtmx:req REQ-TUI-073
    #[test]
    fn test_osc8_wrap_format() {
        let url = "https://example.com";
        let wrapped = osc8_wrap(url);
        assert_eq!(
            wrapped,
            "\x1b]8;;https://example.com\x07https://example.com\x1b]8;;\x07"
        );
    }

    // rtmx:req REQ-TUI-073
    #[test]
    fn test_url_at_end_of_string() {
        let spans = render_with_hyperlinks("Go to https://example.com");
        assert_eq!(spans.len(), 2, "URL at end should produce 2 spans");
        assert_eq!(spans[0].content.as_ref(), "Go to ");
        assert!(spans[1].content.contains("https://example.com"));
    }

    // rtmx:req REQ-TUI-073
    #[test]
    fn test_url_at_start_of_string() {
        let spans = render_with_hyperlinks("https://example.com is the site");
        assert_eq!(spans.len(), 2, "URL at start should produce 2 spans");
        assert!(spans[0].content.contains("https://example.com"));
        assert_eq!(spans[1].content.as_ref(), " is the site");
    }

    // rtmx:req REQ-TUI-073
    #[test]
    fn test_empty_string() {
        let spans = render_with_hyperlinks("");
        assert!(spans.is_empty(), "empty string should produce no spans");
    }

    // rtmx:req REQ-TUI-080
    #[test]
    fn test_url_detection_in_at_trigger() {
        let result = detect_url_in_at_trigger("@url:https://example.com");
        assert_eq!(result, Some("https://example.com"));

        let result = detect_url_in_at_trigger("@url:http://example.com/path?q=1");
        assert_eq!(result, Some("http://example.com/path?q=1"));
    }

    // rtmx:req REQ-TUI-080
    #[test]
    fn test_url_detection_no_prefix() {
        // Without the @url: prefix, detection must return None.
        assert_eq!(detect_url_in_at_trigger("https://example.com"), None);
        assert_eq!(detect_url_in_at_trigger("http://example.com"), None);
        assert_eq!(detect_url_in_at_trigger("plain text"), None);
    }

    // rtmx:req REQ-TUI-080
    #[test]
    fn test_url_detection_invalid_url() {
        assert_eq!(detect_url_in_at_trigger("@url:not-a-url"), None);
        assert_eq!(
            detect_url_in_at_trigger("@url:ftp://files.example.com"),
            None
        );
        assert_eq!(detect_url_in_at_trigger("@url:"), None);
    }

    // rtmx:req REQ-TUI-080
    #[test]
    fn test_url_detection_with_spaces() {
        // Leading/trailing whitespace around the URL should be trimmed.
        let result = detect_url_in_at_trigger("@url: https://example.com ");
        assert_eq!(result, Some("https://example.com"));

        let result = detect_url_in_at_trigger("  @url:  https://example.com  ");
        assert_eq!(result, Some("https://example.com"));
    }

    // rtmx:req REQ-TUI-080
    #[test]
    fn test_is_url_valid() {
        assert!(is_url("http://example.com"));
        assert!(is_url("https://example.com"));
        assert!(is_url("https://example.com/path?key=value#anchor"));
        assert!(is_url("http://localhost:8080"));
    }

    // rtmx:req REQ-TUI-080
    #[test]
    fn test_is_url_invalid() {
        assert!(!is_url("ftp://files.example.com"));
        assert!(!is_url("file:///tmp/test"));
        assert!(!is_url("example.com"));
        assert!(!is_url("not a url at all"));
        assert!(!is_url(""));
    }
}
