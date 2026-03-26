//! Tool output truncation for safety and performance.
//!
//! Large tool outputs can overwhelm the LLM context window and waste tokens.
//! This module truncates outputs that exceed 64KB, appending a marker so the
//! LLM knows the output was cut short.

/// Maximum tool output size in bytes (64 KiB).
const MAX_OUTPUT_BYTES: usize = 64 * 1024;

/// Marker appended when output is truncated.
const TRUNCATION_MARKER: &str = "\n[output truncated at 64KB]";

/// Truncate `output` if it exceeds 64KB, appending a truncation marker.
/// Returns the output unchanged if it fits within the limit.
pub fn truncate_output(output: &str) -> String {
    if output.len() <= MAX_OUTPUT_BYTES {
        return output.to_string();
    }

    // Find a valid UTF-8 boundary at or before MAX_OUTPUT_BYTES.
    let mut cut = MAX_OUTPUT_BYTES;
    while cut > 0 && !output.is_char_boundary(cut) {
        cut -= 1;
    }

    let mut truncated = output[..cut].to_string();
    truncated.push_str(TRUNCATION_MARKER);
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    // @req REQ-AGENT-012
    #[test]
    fn small_output_passes_through() {
        let input = "hello world";
        let result = truncate_output(input);
        assert_eq!(result, input);
    }

    // @req REQ-AGENT-012
    #[test]
    fn exact_limit_passes_through() {
        let input = "x".repeat(MAX_OUTPUT_BYTES);
        let result = truncate_output(&input);
        assert_eq!(result.len(), MAX_OUTPUT_BYTES);
        assert!(!result.contains("[output truncated"));
    }

    // @req REQ-AGENT-012
    #[test]
    fn over_limit_is_truncated() {
        let input = "a".repeat(MAX_OUTPUT_BYTES + 1000);
        let result = truncate_output(&input);
        assert!(result.len() < input.len());
        assert!(result.ends_with("[output truncated at 64KB]"));
    }

    // @req REQ-AGENT-012
    #[test]
    fn truncation_preserves_utf8_boundary() {
        // Create a string with multi-byte chars that would split at the boundary.
        // Each char is 4 bytes.
        let emoji = "X".repeat(MAX_OUTPUT_BYTES - 2) + "\u{1F600}\u{1F600}";
        let result = truncate_output(&emoji);
        // Must be valid UTF-8 -- if this panics, the boundary check failed.
        assert!(result.ends_with("[output truncated at 64KB]"));
        // Verify the truncated content (before marker) is valid UTF-8.
        let before_marker = result.strip_suffix(TRUNCATION_MARKER).unwrap();
        assert!(before_marker.len() <= MAX_OUTPUT_BYTES);
    }

    // @req REQ-AGENT-012
    #[test]
    fn empty_string_passes_through() {
        let result = truncate_output("");
        assert_eq!(result, "");
    }

    // @req REQ-AGENT-012
    #[test]
    fn truncated_output_starts_with_original_prefix() {
        let input = "abcdef".repeat(MAX_OUTPUT_BYTES);
        let result = truncate_output(&input);
        let before_marker = result.strip_suffix(TRUNCATION_MARKER).unwrap();
        assert!(input.starts_with(before_marker));
    }
}
