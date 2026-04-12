//! SHA-256 hash chain for audit ledger integrity verification.
//!
//! Each ledger entry carries a `prev_hash` (the hash of the preceding entry)
//! and an `entry_hash` (SHA-256 of prev_hash concatenated with the JSON of the
//! entry excluding hash fields). The genesis entry uses a zero hash as its
//! `prev_hash`.

use sha2::{Digest, Sha256};
use std::fmt;

/// The zero hash used as `prev_hash` for the genesis (first) entry.
pub const GENESIS_PREV_HASH: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

/// Errors that can occur during chain verification.
#[derive(Debug, PartialEq, Eq)]
pub enum ChainError {
    /// A JSON line could not be parsed.
    InvalidJson { line: usize, message: String },
    /// The `prev_hash` field is missing from an entry.
    MissingPrevHash { line: usize },
    /// The `entry_hash` field is missing from an entry.
    MissingEntryHash { line: usize },
    /// The `prev_hash` of entry N does not match the `entry_hash` of entry N-1.
    BrokenLink {
        line: usize,
        expected: String,
        actual: String,
    },
    /// The `entry_hash` does not match the recomputed hash of the entry body.
    TamperedEntry {
        line: usize,
        expected: String,
        actual: String,
    },
}

impl fmt::Display for ChainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChainError::InvalidJson { line, message } => {
                write!(f, "line {line}: invalid JSON: {message}")
            }
            ChainError::MissingPrevHash { line } => {
                write!(f, "line {line}: missing prev_hash field")
            }
            ChainError::MissingEntryHash { line } => {
                write!(f, "line {line}: missing entry_hash field")
            }
            ChainError::BrokenLink {
                line,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "line {line}: broken chain link: expected prev_hash \
                     {expected}, got {actual}"
                )
            }
            ChainError::TamperedEntry {
                line,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "line {line}: tampered entry: expected entry_hash \
                     {expected}, got {actual}"
                )
            }
        }
    }
}

impl std::error::Error for ChainError {}

/// Compute the SHA-256 hash of `prev_hash` concatenated with the JSON body
/// (the entry with `prev_hash` and `entry_hash` fields removed).
pub fn compute_entry_hash(prev_hash: &str, json_line: &str) -> String {
    let mut parsed: serde_json::Value =
        serde_json::from_str(json_line).expect("json_line must be valid JSON");

    // Remove hash fields before hashing
    if let Some(obj) = parsed.as_object_mut() {
        obj.remove("prev_hash");
        obj.remove("entry_hash");
    }

    let body = serde_json::to_string(&parsed).expect("re-serialization cannot fail");
    let mut hasher = Sha256::new();
    hasher.update(prev_hash.as_bytes());
    hasher.update(body.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Verify the integrity of a hash chain given a slice of JSONL strings.
///
/// An empty chain is valid.
///
/// Each entry must contain `prev_hash` and `entry_hash` fields. The genesis
/// entry must have `prev_hash` equal to [`GENESIS_PREV_HASH`]. Each subsequent
/// entry's `prev_hash` must equal the preceding entry's `entry_hash`.
/// Every entry's `entry_hash` must match the recomputed hash.
pub fn verify_chain(entries: &[String]) -> Result<(), ChainError> {
    if entries.is_empty() {
        return Ok(());
    }

    let mut last_hash = GENESIS_PREV_HASH.to_string();

    for (i, line) in entries.iter().enumerate() {
        let line_num = i + 1;

        let parsed: serde_json::Value =
            serde_json::from_str(line).map_err(|e| ChainError::InvalidJson {
                line: line_num,
                message: e.to_string(),
            })?;

        let prev_hash = parsed
            .get("prev_hash")
            .and_then(|v| v.as_str())
            .ok_or(ChainError::MissingPrevHash { line: line_num })?;

        let entry_hash = parsed
            .get("entry_hash")
            .and_then(|v| v.as_str())
            .ok_or(ChainError::MissingEntryHash { line: line_num })?;

        // Verify the chain link
        if prev_hash != last_hash {
            return Err(ChainError::BrokenLink {
                line: line_num,
                expected: last_hash,
                actual: prev_hash.to_string(),
            });
        }

        // Verify the entry hash
        let recomputed = compute_entry_hash(prev_hash, line);
        if entry_hash != recomputed {
            return Err(ChainError::TamperedEntry {
                line: line_num,
                expected: recomputed,
                actual: entry_hash.to_string(),
            });
        }

        last_hash = entry_hash.to_string();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Build a valid chain of N entries, returning JSONL strings.
    fn build_chain(n: usize) -> Vec<String> {
        let mut entries = Vec::new();
        let mut prev = GENESIS_PREV_HASH.to_string();

        for i in 0..n {
            let body = json!({
                "timestamp": format!("2026-03-25T00:00:0{i}Z"),
                "os_user": "testuser",
                "hostname": "testhost",
                "event": {"type": "test", "index": i}
            });

            let mut obj = body.as_object().unwrap().clone();
            obj.insert("prev_hash".to_string(), json!(prev));

            // Compute hash over prev_hash + body (without hash fields)
            let body_json = serde_json::to_string(&body).unwrap();
            let temp_line = format!(
                "{{\"prev_hash\":\"{prev}\",{}}}",
                &body_json[1..body_json.len() - 1]
            );
            let hash = compute_entry_hash(&prev, &temp_line);

            obj.insert("entry_hash".to_string(), json!(hash));
            let line = serde_json::to_string(&obj).unwrap();
            entries.push(line);
            prev = hash;
        }

        entries
    }

    // rtmx:req REQ-AUDIT-005
    #[test]
    fn empty_chain_is_valid() {
        let entries: Vec<String> = vec![];
        assert!(verify_chain(&entries).is_ok());
    }

    // rtmx:req REQ-AUDIT-005
    #[test]
    fn single_entry_chain_is_valid() {
        let chain = build_chain(1);
        assert!(verify_chain(&chain).is_ok());
    }

    // rtmx:req REQ-AUDIT-005
    #[test]
    fn multi_entry_chain_is_valid() {
        let chain = build_chain(5);
        assert!(verify_chain(&chain).is_ok());
    }

    // rtmx:req REQ-AUDIT-005
    #[test]
    fn genesis_entry_has_zero_prev_hash() {
        let chain = build_chain(1);
        let parsed: serde_json::Value = serde_json::from_str(&chain[0]).unwrap();
        assert_eq!(parsed["prev_hash"].as_str().unwrap(), GENESIS_PREV_HASH);
    }

    // rtmx:req REQ-AUDIT-005
    #[test]
    fn tampered_entry_detected() {
        let mut chain = build_chain(3);

        // Tamper with the second entry's event data
        let mut parsed: serde_json::Value = serde_json::from_str(&chain[1]).unwrap();
        parsed["event"]["type"] = json!("TAMPERED");
        chain[1] = serde_json::to_string(&parsed).unwrap();

        let result = verify_chain(&chain);
        assert!(result.is_err());
        match result.unwrap_err() {
            ChainError::TamperedEntry { line, .. } => {
                assert_eq!(line, 2);
            }
            other => panic!("Expected TamperedEntry, got: {other:?}"),
        }
    }

    // rtmx:req REQ-AUDIT-005
    #[test]
    fn broken_link_detected() {
        let mut chain = build_chain(3);

        // Replace the third entry's prev_hash with garbage
        let mut parsed: serde_json::Value = serde_json::from_str(&chain[2]).unwrap();
        parsed["prev_hash"] = json!("deadbeef");
        chain[2] = serde_json::to_string(&parsed).unwrap();

        let result = verify_chain(&chain);
        assert!(result.is_err());
        match result.unwrap_err() {
            ChainError::BrokenLink { line, .. } => {
                assert_eq!(line, 3);
            }
            other => panic!("Expected BrokenLink, got: {other:?}"),
        }
    }

    // rtmx:req REQ-AUDIT-005
    #[test]
    fn missing_prev_hash_detected() {
        let entry = json!({
            "timestamp": "2026-03-25T00:00:00Z",
            "entry_hash": "abc",
            "event": {"type": "test"}
        });
        let entries = vec![serde_json::to_string(&entry).unwrap()];

        let result = verify_chain(&entries);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ChainError::MissingPrevHash { line: 1 }
        ));
    }

    // rtmx:req REQ-AUDIT-005
    #[test]
    fn missing_entry_hash_detected() {
        let entry = json!({
            "timestamp": "2026-03-25T00:00:00Z",
            "prev_hash": GENESIS_PREV_HASH,
            "event": {"type": "test"}
        });
        let entries = vec![serde_json::to_string(&entry).unwrap()];

        let result = verify_chain(&entries);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ChainError::MissingEntryHash { line: 1 }
        ));
    }

    // rtmx:req REQ-AUDIT-005
    #[test]
    fn invalid_json_detected() {
        let entries = vec!["not valid json".to_string()];
        let result = verify_chain(&entries);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ChainError::InvalidJson { line: 1, .. }
        ));
    }

    // rtmx:req REQ-AUDIT-005
    #[test]
    fn compute_entry_hash_is_deterministic() {
        let line = json!({
            "prev_hash": GENESIS_PREV_HASH,
            "entry_hash": "will_be_stripped",
            "timestamp": "2026-03-25T00:00:00Z",
            "event": {"type": "test"}
        });
        let line_str = serde_json::to_string(&line).unwrap();

        let hash1 = compute_entry_hash(GENESIS_PREV_HASH, &line_str);
        let hash2 = compute_entry_hash(GENESIS_PREV_HASH, &line_str);
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64, "SHA-256 hex digest should be 64 chars");
    }
}
