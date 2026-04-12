//! Response validation for LLM output.

use std::fmt;

/// Maximum allowed response size: 1 MB.
const MAX_RESPONSE_BYTES: usize = 1_048_576;

/// Errors that can occur when validating an LLM response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    Empty,
    ContainsNullBytes,
    TooLarge { size: usize, max: usize },
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationError::Empty => write!(f, "response is empty"),
            ValidationError::ContainsNullBytes => {
                write!(f, "response contains null bytes")
            }
            ValidationError::TooLarge { size, max } => {
                write!(
                    f,
                    "response too large: {size} bytes exceeds {max} byte limit"
                )
            }
        }
    }
}

impl std::error::Error for ValidationError {}

/// Validate an LLM response text. Rejects empty responses, responses
/// containing null bytes, and responses exceeding 1 MB.
pub fn validate_response(text: &str) -> Result<(), ValidationError> {
    if text.is_empty() {
        return Err(ValidationError::Empty);
    }
    if text.contains('\0') {
        return Err(ValidationError::ContainsNullBytes);
    }
    if text.len() > MAX_RESPONSE_BYTES {
        return Err(ValidationError::TooLarge {
            size: text.len(),
            max: MAX_RESPONSE_BYTES,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-LLM-013
    #[test]
    fn rejects_empty_response() {
        assert_eq!(validate_response(""), Err(ValidationError::Empty));
    }

    // rtmx:req REQ-LLM-013
    #[test]
    fn rejects_null_bytes() {
        assert_eq!(
            validate_response("hello\0world"),
            Err(ValidationError::ContainsNullBytes)
        );
    }

    // rtmx:req REQ-LLM-013
    #[test]
    fn rejects_response_over_1mb() {
        let large = "x".repeat(MAX_RESPONSE_BYTES + 1);
        match validate_response(&large) {
            Err(ValidationError::TooLarge { size, max }) => {
                assert_eq!(size, MAX_RESPONSE_BYTES + 1);
                assert_eq!(max, MAX_RESPONSE_BYTES);
            }
            other => panic!("Expected TooLarge, got {other:?}"),
        }
    }

    // rtmx:req REQ-LLM-013
    #[test]
    fn accepts_valid_response() {
        assert!(validate_response("Hello, world!").is_ok());
    }

    // rtmx:req REQ-LLM-013
    #[test]
    fn accepts_response_exactly_at_limit() {
        let at_limit = "x".repeat(MAX_RESPONSE_BYTES);
        assert!(validate_response(&at_limit).is_ok());
    }

    // rtmx:req REQ-LLM-013
    #[test]
    fn rejects_null_byte_at_start() {
        assert_eq!(
            validate_response("\0hello"),
            Err(ValidationError::ContainsNullBytes)
        );
    }

    // rtmx:req REQ-LLM-013
    #[test]
    fn rejects_null_byte_at_end() {
        assert_eq!(
            validate_response("hello\0"),
            Err(ValidationError::ContainsNullBytes)
        );
    }

    // rtmx:req REQ-LLM-013
    #[test]
    fn error_display_messages() {
        assert_eq!(ValidationError::Empty.to_string(), "response is empty");
        assert_eq!(
            ValidationError::ContainsNullBytes.to_string(),
            "response contains null bytes"
        );
        assert_eq!(
            ValidationError::TooLarge {
                size: 2_000_000,
                max: 1_048_576
            }
            .to_string(),
            "response too large: 2000000 bytes exceeds 1048576 byte limit"
        );
    }
}
