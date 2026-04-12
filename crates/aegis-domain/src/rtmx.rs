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
    pub effort_weeks: String,
    #[serde(default)]
    pub dependencies: String,
    #[serde(default)]
    pub blocks: String,
    #[serde(default)]
    pub assignee: String,
    #[serde(default)]
    pub sprint: String,
    #[serde(default)]
    pub started_date: String,
    #[serde(default)]
    pub completed_date: String,
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
                effort_weeks: get("effort_weeks"),
                dependencies: get("dependencies"),
                blocks: get("blocks"),
                assignee: get("assignee"),
                sprint: get("sprint"),
                started_date: get("started_date"),
                completed_date: get("completed_date"),
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

    /// Get a mutable reference to a requirement by ID.
    fn get_mut(&mut self, req_id: &str) -> Result<&mut Requirement, DomainError> {
        let &idx = self
            .by_id
            .get(req_id)
            .ok_or_else(|| DomainError::RequirementNotFound {
                id: req_id.to_string(),
            })?;
        Ok(&mut self.requirements[idx])
    }

    /// Update the status field for a requirement.
    pub fn update_status(&mut self, req_id: &str, new_status: &str) -> Result<(), DomainError> {
        let req = self.get_mut(req_id)?;
        req.status = new_status.to_string();
        Ok(())
    }

    /// Update the test_module and test_function fields for a requirement.
    pub fn update_test_info(
        &mut self,
        req_id: &str,
        test_module: &str,
        test_function: &str,
    ) -> Result<(), DomainError> {
        let req = self.get_mut(req_id)?;
        req.test_module = test_module.to_string();
        req.test_function = test_function.to_string();
        Ok(())
    }

    /// Set a requirement to COMPLETE with today's date.
    pub fn set_completed(&mut self, req_id: &str) -> Result<(), DomainError> {
        let req = self.get_mut(req_id)?;
        req.status = "COMPLETE".to_string();
        req.completed_date = chrono::Utc::now().format("%Y-%m-%d").to_string();
        Ok(())
    }

    /// Write the current state back to a CSV file.
    pub fn save_csv(&self, path: &Path) -> Result<(), DomainError> {
        if let Some(parent) = path.parent().filter(|p| !p.exists()) {
            std::fs::create_dir_all(parent).map_err(|e| {
                DomainError::Other(format!(
                    "Failed to create directory {}: {e}",
                    parent.display()
                ))
            })?;
        }

        let mut out = String::new();
        out.push_str(CSV_HEADER);
        out.push('\n');

        for req in &self.requirements {
            let row = [
                &req.req_id,
                &req.category,
                &req.subcategory,
                &req.requirement_text,
                &req.target_value,
                &req.test_module,
                &req.test_function,
                &req.validation_method,
                &req.status,
                &req.priority,
                &req.phase,
                &req.notes,
                &req.effort_weeks,
                &req.dependencies,
                &req.blocks,
                &req.assignee,
                &req.sprint,
                &req.started_date,
                &req.completed_date,
            ];
            let formatted: Vec<String> = row
                .iter()
                .map(|f| {
                    if f.contains(',') || f.contains('"') {
                        format!("\"{}\"", f.replace('"', "\"\""))
                    } else {
                        f.to_string()
                    }
                })
                .collect();
            out.push_str(&formatted.join(","));
            out.push('\n');
        }

        std::fs::write(path, out)
            .map_err(|e| DomainError::Other(format!("Failed to write {}: {e}", path.display())))
    }
}

/// CSV header matching the full RTMX database schema.
const CSV_HEADER: &str = "req_id,category,subcategory,requirement_text,\
    target_value,test_module,test_function,validation_method,status,\
    priority,phase,notes,effort_weeks,dependencies,blocks,assignee,\
    sprint,started_date,completed_date";

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

    // rtmx:req REQ-RTMX-001
    #[test]
    fn parse_csv_returns_all_requirements() {
        let db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        assert_eq!(db.count(), 3);
    }

    // rtmx:req REQ-RTMX-001
    #[test]
    fn get_requirement_by_id() {
        let db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        let req = db.get("REQ-BUILD-001").unwrap();
        assert_eq!(req.category, "BUILD");
        assert_eq!(req.status, "COMPLETE");
    }

    // rtmx:req REQ-RTMX-001
    #[test]
    fn get_nonexistent_returns_none() {
        let db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        assert!(db.get("REQ-FAKE-999").is_none());
    }

    // rtmx:req REQ-RTMX-001
    #[test]
    fn filter_by_category() {
        let db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        let build = db.by_category("BUILD");
        assert_eq!(build.len(), 1);
        assert_eq!(build[0].req_id, "REQ-BUILD-001");
    }

    // rtmx:req REQ-RTMX-001
    #[test]
    fn filter_by_status() {
        let db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        let complete = db.by_status("COMPLETE");
        assert_eq!(complete.len(), 2);
    }

    // rtmx:req REQ-RTMX-001
    #[test]
    fn count_by_status() {
        let db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        assert_eq!(db.count_by_status("COMPLETE"), 2);
        assert_eq!(db.count_by_status("TODO"), 1);
    }

    // rtmx:req REQ-RTMX-001
    #[test]
    fn parse_dependencies() {
        let db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        let req = db.get("REQ-AGENT-001").unwrap();
        assert_eq!(req.dependencies, "REQ-LLM-001");
    }

    // rtmx:req REQ-RTMX-001
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

    // rtmx:req REQ-RTMX-001
    #[test]
    fn empty_csv_returns_error() {
        let result = RequirementsDb::from_csv("");
        assert!(result.is_err());
    }

    // rtmx:req REQ-RTMX-001
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

    // rtmx:req REQ-RTMX-002
    #[test]
    fn update_status_changes_the_field() {
        let mut db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        db.update_status("REQ-TUI-001", "IN_PROGRESS").unwrap();
        assert_eq!(db.get("REQ-TUI-001").unwrap().status, "IN_PROGRESS");
    }

    // rtmx:req REQ-RTMX-002
    #[test]
    fn update_status_nonexistent_req_returns_error() {
        let mut db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        let result = db.update_status("REQ-FAKE-999", "DONE");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("REQ-FAKE-999"),
            "Error should mention the missing req_id"
        );
    }

    // rtmx:req REQ-RTMX-002
    #[test]
    fn update_test_info_sets_both_fields() {
        let mut db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        db.update_test_info("REQ-TUI-001", "tests/tui/new.rs", "test_new_layout")
            .unwrap();
        let req = db.get("REQ-TUI-001").unwrap();
        assert_eq!(req.test_module, "tests/tui/new.rs");
        assert_eq!(req.test_function, "test_new_layout");
    }

    // rtmx:req REQ-RTMX-002
    #[test]
    fn set_completed_updates_status_and_date() {
        let mut db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        db.set_completed("REQ-TUI-001").unwrap();
        let req = db.get("REQ-TUI-001").unwrap();
        assert_eq!(req.status, "COMPLETE");
        // Date should be today in YYYY-MM-DD format.
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        assert_eq!(req.completed_date, today);
    }

    // rtmx:req REQ-RTMX-002
    #[test]
    fn save_csv_roundtrips_correctly() {
        let mut db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        db.update_status("REQ-TUI-001", "IN_PROGRESS").unwrap();

        let dir = std::env::temp_dir().join("aegis_test_roundtrip");
        let path = dir.join("database.csv");
        db.save_csv(&path).unwrap();

        let db2 = RequirementsDb::load(&path).unwrap();
        assert_eq!(db2.count(), 3);
        assert_eq!(db2.get("REQ-TUI-001").unwrap().status, "IN_PROGRESS");
        assert_eq!(db2.get("REQ-BUILD-001").unwrap().status, "COMPLETE");

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }

    // rtmx:req REQ-RTMX-002
    #[test]
    fn save_preserves_all_columns() {
        let db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        let dir = std::env::temp_dir().join("aegis_test_columns");
        let path = dir.join("database.csv");
        db.save_csv(&path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        // Header should contain all 19 column names
        let header_line = content.lines().next().unwrap();
        for col in &[
            "req_id",
            "category",
            "subcategory",
            "requirement_text",
            "target_value",
            "test_module",
            "test_function",
            "validation_method",
            "status",
            "priority",
            "phase",
            "notes",
            "effort_weeks",
            "dependencies",
            "blocks",
            "assignee",
            "sprint",
            "started_date",
            "completed_date",
        ] {
            assert!(header_line.contains(col), "Header missing column: {col}");
        }
        // Data rows preserved
        assert!(content.contains("REQ-BUILD-001"));
        assert!(content.contains("REQ-AGENT-001"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    // rtmx:req REQ-RTMX-002
    #[test]
    fn multiple_updates_accumulate() {
        let mut db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        db.update_status("REQ-TUI-001", "IN_PROGRESS").unwrap();
        db.update_test_info("REQ-TUI-001", "tests/tui/v2.rs", "test_v2")
            .unwrap();
        db.update_status("REQ-TUI-001", "COMPLETE").unwrap();

        let req = db.get("REQ-TUI-001").unwrap();
        assert_eq!(req.status, "COMPLETE");
        assert_eq!(req.test_module, "tests/tui/v2.rs");
        assert_eq!(req.test_function, "test_v2");
    }

    // rtmx:req REQ-RTMX-002
    #[test]
    fn save_creates_parent_directory_if_missing() {
        let dir = std::env::temp_dir()
            .join("aegis_test_mkdir")
            .join("nested")
            .join("deep");
        let path = dir.join("database.csv");

        // Ensure it does not exist
        let _ = std::fs::remove_dir_all(std::env::temp_dir().join("aegis_test_mkdir"));

        let db = RequirementsDb::from_csv(SAMPLE_CSV).unwrap();
        db.save_csv(&path).unwrap();

        assert!(path.exists(), "CSV file should have been created");
        let db2 = RequirementsDb::load(&path).unwrap();
        assert_eq!(db2.count(), 3);

        let _ = std::fs::remove_dir_all(std::env::temp_dir().join("aegis_test_mkdir"));
    }
}
