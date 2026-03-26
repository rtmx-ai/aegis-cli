//! RTMX requirement types and CSV parser.
//!
//! Reads requirements from .rtmx/database.csv and exposes them
//! as queryable domain objects for the agent loop.

use crate::error::DomainError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// A single RTMX requirement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Requirement {
    pub req_id: String,
    pub category: String,
    pub subcategory: String,
    pub requirement_text: String,
    pub target_value: String,
    pub test_module: String,
    pub test_function: String,
    pub validation_method: String,
    pub status: String,
    pub priority: String,
    pub phase: String,
    pub notes: String,
    #[serde(default)]
    pub dependencies: String,
}

/// A parsed RTMX requirements database.
#[derive(Debug, Clone)]
pub struct RequirementsDb {
    requirements: Vec<Requirement>,
    by_id: HashMap<String, usize>,
}

impl RequirementsDb {
    /// Parse requirements from a CSV string.
    pub fn from_csv(csv_content: &str) -> Result<Self, DomainError> {
        let mut requirements = Vec::new();
        let mut lines = csv_content.lines();

        // Parse header
        let header = lines
            .next()
            .ok_or_else(|| DomainError::Other("Empty CSV file".to_string()))?;
        let columns: Vec<&str> = parse_csv_row(header);

        let col_index = |name: &str| -> Option<usize> { columns.iter().position(|c| *c == name) };

        let id_col = col_index("req_id")
            .ok_or_else(|| DomainError::Other("Missing req_id column".to_string()))?;

        for line in lines {
            if line.trim().is_empty() {
                continue;
            }
            let fields = parse_csv_row(line);
            if fields.len() <= id_col {
                continue;
            }

            let get = |name: &str| -> String {
                col_index(name)
                    .and_then(|i| fields.get(i))
                    .map(|s| s.to_string())
                    .unwrap_or_default()
            };

            requirements.push(Requirement {
                req_id: get("req_id"),
                category: get("category"),
                subcategory: get("subcategory"),
                requirement_text: get("requirement_text"),
                target_value: get("target_value"),
                test_module: get("test_module"),
                test_function: get("test_function"),
                validation_method: get("validation_method"),
                status: get("status"),
                priority: get("priority"),
                phase: get("phase"),
                notes: get("notes"),
                dependencies: get("dependencies"),
            });
        }

        let by_id: HashMap<String, usize> = requirements
            .iter()
            .enumerate()
            .map(|(i, r)| (r.req_id.clone(), i))
            .collect();

        Ok(Self {
            requirements,
            by_id,
        })
    }

    /// Load requirements from a CSV file path.
    pub fn load(path: &Path) -> Result<Self, DomainError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| DomainError::Other(format!("Failed to read {}: {e}", path.display())))?;
        Self::from_csv(&content)
    }

    /// Get a requirement by ID.
    pub fn get(&self, req_id: &str) -> Option<&Requirement> {
        self.by_id.get(req_id).map(|&i| &self.requirements[i])
    }

    /// Get all requirements.
    pub fn all(&self) -> &[Requirement] {
        &self.requirements
    }

    /// Get requirements by category.
    pub fn by_category(&self, category: &str) -> Vec<&Requirement> {
        self.requirements
            .iter()
            .filter(|r| r.category == category)
            .collect()
    }

    /// Get requirements by status.
    pub fn by_status(&self, status: &str) -> Vec<&Requirement> {
        self.requirements
            .iter()
            .filter(|r| r.status == status)
            .collect()
    }

    /// Count total requirements.
    pub fn count(&self) -> usize {
        self.requirements.len()
    }

    /// Count requirements by status.
    pub fn count_by_status(&self, status: &str) -> usize {
        self.requirements
            .iter()
            .filter(|r| r.status == status)
            .count()
    }
}

/// Simple CSV row parser that handles quoted fields with commas.
fn parse_csv_row(line: &str) -> Vec<&str> {
    let mut fields = Vec::new();
    let mut start = 0;
    let mut in_quotes = false;
    let bytes = line.as_bytes();

    for i in 0..bytes.len() {
        match bytes[i] {
            b'"' => in_quotes = !in_quotes,
            b',' if !in_quotes => {
                let field = &line[start..i];
                fields.push(field.trim_matches('"'));
                start = i + 1;
            }
            _ => {}
        }
    }
    // Last field
    let field = &line[start..];
    fields.push(field.trim_matches('"'));

    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_CSV: &str = "\
req_id,category,subcategory,requirement_text,target_value,test_module,test_function,validation_method,status,priority,phase,notes,dependencies
REQ-BUILD-001,BUILD,BINARY,Static binary,Runs on RHEL,tests/build.rs,test_binary,System Test,COMPLETE,CRITICAL,1,Rust musl,
REQ-TUI-001,TUI,LAYOUT,Chat layout,TUI renders,tests/tui.rs,test_layout,Unit Test,TODO,CRITICAL,1,ratatui,
REQ-AGENT-001,AGENT,LOOP,REA loop,Agent completes,tests/agent.rs,test_loop,Integration Test,COMPLETE,CRITICAL,1,Goose fork,REQ-LLM-001";

    // @req REQ-RTMX-001
    #[test]
    fn parse_csv_returns_all_requirements() {
        let db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        assert_eq!(db.count(), 3);
    }

    // @req REQ-RTMX-001
    #[test]
    fn get_requirement_by_id() {
        let db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        let req = db.get("REQ-BUILD-001").unwrap();
        assert_eq!(req.category, "BUILD");
        assert_eq!(req.status, "COMPLETE");
    }

    // @req REQ-RTMX-001
    #[test]
    fn get_nonexistent_returns_none() {
        let db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        assert!(db.get("REQ-FAKE-999").is_none());
    }

    // @req REQ-RTMX-001
    #[test]
    fn filter_by_category() {
        let db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        let build = db.by_category("BUILD");
        assert_eq!(build.len(), 1);
        assert_eq!(build[0].req_id, "REQ-BUILD-001");
    }

    // @req REQ-RTMX-001
    #[test]
    fn filter_by_status() {
        let db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        let complete = db.by_status("COMPLETE");
        assert_eq!(complete.len(), 2);
    }

    // @req REQ-RTMX-001
    #[test]
    fn count_by_status() {
        let db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        assert_eq!(db.count_by_status("COMPLETE"), 2);
        assert_eq!(db.count_by_status("TODO"), 1);
    }

    // @req REQ-RTMX-001
    #[test]
    fn parse_dependencies() {
        let db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        let req = db.get("REQ-AGENT-001").unwrap();
        assert_eq!(req.dependencies, "REQ-LLM-001");
    }

    // @req REQ-RTMX-001
    #[test]
    fn handles_quoted_fields_with_commas() {
        let csv = "\
req_id,category,subcategory,requirement_text,target_value,test_module,test_function,validation_method,status,priority,phase,notes,dependencies
REQ-TEST-001,TEST,X,\"Requirement with, comma\",\"Target with, comma\",t.rs,test_fn,Unit Test,TODO,HIGH,1,\"Notes, here\",";
        let db = RequirementsDb::from_csv(csv).unwrap();
        let req = db.get("REQ-TEST-001").unwrap();
        assert_eq!(req.requirement_text, "Requirement with, comma");
        assert_eq!(req.target_value, "Target with, comma");
    }

    // @req REQ-RTMX-001
    #[test]
    fn empty_csv_returns_error() {
        let result = RequirementsDb::from_csv("");
        assert!(result.is_err());
    }

    // @req REQ-RTMX-001
    #[test]
    fn loads_real_database() {
        let path = std::path::Path::new(".rtmx/database.csv");
        if path.exists() {
            let db = RequirementsDb::load(path).unwrap();
            assert!(
                db.count() > 100,
                "Real database should have 100+ requirements, got {}",
                db.count()
            );
            // Verify we can find a known requirement
            assert!(db.get("REQ-BUILD-001").is_some());
        }
    }
}
