//! RTMX requirements picker for @req context injection.
//!
//! When the user types `@req:` in the input field, a dropdown appears showing
//! requirements from `.rtmx/database.csv`. The picker supports filtering by
//! REQ-ID or description text, and shows a preview pane with full requirement
//! details (status, dependencies, notes).

use std::path::Path;

/// A single requirement entry parsed from `.rtmx/database.csv`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequirementEntry {
    pub req_id: String,
    pub category: String,
    pub description: String,
    pub status: String,
    pub priority: String,
    pub dependencies: String,
    pub notes: String,
}

/// Load requirements from `.rtmx/database.csv` in the given directory.
///
/// The `rtmx_dir` should point to the `.rtmx` directory containing
/// `database.csv`. Returns an empty vec if the file cannot be read or parsed.
pub fn load_requirements(rtmx_dir: &Path) -> Vec<RequirementEntry> {
    let csv_path = rtmx_dir.join("database.csv");
    let content = match std::fs::read_to_string(&csv_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut entries = Vec::new();
    let mut lines = content.lines();

    // Skip header row.
    let header = match lines.next() {
        Some(h) => h,
        None => return Vec::new(),
    };

    // Build column index map from header.
    let columns: Vec<&str> = header.split(',').collect();
    let idx = |name: &str| columns.iter().position(|c| *c == name);

    let i_req_id = match idx("req_id") {
        Some(i) => i,
        None => return Vec::new(),
    };
    let i_category = idx("category");
    let i_description = idx("requirement_text");
    let i_status = idx("status");
    let i_priority = idx("priority");
    let i_dependencies = idx("dependencies");
    let i_notes = idx("notes");

    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split(',').collect();
        let get = |opt_idx: Option<usize>| -> String {
            opt_idx
                .and_then(|i| fields.get(i))
                .unwrap_or(&"")
                .to_string()
        };

        let req_id = match fields.get(i_req_id) {
            Some(id) if !id.is_empty() => id.to_string(),
            _ => continue,
        };

        entries.push(RequirementEntry {
            req_id,
            category: get(i_category),
            description: get(i_description),
            status: get(i_status),
            priority: get(i_priority),
            dependencies: get(i_dependencies),
            notes: get(i_notes),
        });
    }

    entries
}

/// Filter requirements by a case-insensitive substring match on req_id or
/// description text.
pub fn filter_requirements<'a>(
    entries: &'a [RequirementEntry],
    query: &str,
) -> Vec<&'a RequirementEntry> {
    if query.is_empty() {
        return entries.iter().collect();
    }
    let lower = query.to_lowercase();
    entries
        .iter()
        .filter(|e| {
            e.req_id.to_lowercase().contains(&lower)
                || e.description.to_lowercase().contains(&lower)
        })
        .collect()
}

/// Format a full preview of a requirement entry for the preview pane.
pub fn format_preview(entry: &RequirementEntry) -> String {
    let mut lines = Vec::new();
    lines.push(entry.req_id.to_string());
    lines.push(format!("Category: {}", entry.category));
    lines.push(format!("Description: {}", entry.description));
    lines.push(format!("Status: {}", entry.status));
    lines.push(format!("Priority: {}", entry.priority));
    if !entry.dependencies.is_empty() {
        lines.push(format!("Dependencies: {}", entry.dependencies));
    }
    if !entry.notes.is_empty() {
        lines.push(format!("Notes: {}", entry.notes));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Create a temporary `.rtmx/database.csv` with test data.
    fn create_test_database(dir: &Path) -> std::path::PathBuf {
        let rtmx_dir = dir.join(".rtmx");
        fs::create_dir_all(&rtmx_dir).unwrap();
        let csv = rtmx_dir.join("database.csv");
        fs::write(
            &csv,
            "req_id,category,subcategory,requirement_text,target_value,\
             test_module,test_function,validation_method,status,priority,\
             phase,notes,effort_weeks,dependencies,blocks,assignee,sprint,\
             started_date,completed_date,requirement_file,external_id\n\
             REQ-AUDIT-001,AUDIT,LEDGER,Immutable JSONL audit ledger,\
             Ledger appends only,mod.rs,test_append,Unit Test,COMPLETE,HIGH,\
             1,Append-only JSONL file,1,,,,,,,\n\
             REQ-AUDIT-002,AUDIT,INTEGRITY,Tamper detection on audit log,\
             SHA-256 hash chain,mod.rs,test_integrity,Unit Test,IN_PROGRESS,\
             MEDIUM,1,Hash chain verification,2,REQ-AUDIT-001,,,,,,\n\
             REQ-TUI-047,TUI,PICKER,Interactive file picker with clipboard,\
             Picker opens on @ key,mod.rs,test_picker,Unit Test,COMPLETE,LOW,\
             1,Uses clipboard for paste,0.5,,,,,,,\n",
        )
        .unwrap();
        rtmx_dir
    }

    // rtmx:req REQ-TUI-051
    #[test]
    fn test_req_trigger_lists_requirements() {
        let tmp = TempDir::new().unwrap();
        let rtmx_dir = create_test_database(tmp.path());
        let entries = load_requirements(&rtmx_dir);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].req_id, "REQ-AUDIT-001");
        assert_eq!(entries[1].req_id, "REQ-AUDIT-002");
        assert_eq!(entries[2].req_id, "REQ-TUI-047");
    }

    // rtmx:req REQ-TUI-051
    #[test]
    fn test_req_filter_by_id() {
        let tmp = TempDir::new().unwrap();
        let rtmx_dir = create_test_database(tmp.path());
        let entries = load_requirements(&rtmx_dir);
        let filtered = filter_requirements(&entries, "AUDIT");
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|e| e.req_id.contains("AUDIT")));
    }

    // rtmx:req REQ-TUI-051
    #[test]
    fn test_req_filter_by_description() {
        let tmp = TempDir::new().unwrap();
        let rtmx_dir = create_test_database(tmp.path());
        let entries = load_requirements(&rtmx_dir);
        let filtered = filter_requirements(&entries, "clipboard");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].req_id, "REQ-TUI-047");
    }

    // rtmx:req REQ-TUI-051
    #[test]
    fn test_req_filter_case_insensitive() {
        let tmp = TempDir::new().unwrap();
        let rtmx_dir = create_test_database(tmp.path());
        let entries = load_requirements(&rtmx_dir);
        let filtered = filter_requirements(&entries, "audit");
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|e| e.req_id.contains("AUDIT")));
    }

    // rtmx:req REQ-TUI-051
    #[test]
    fn test_req_preview_shows_details() {
        let entry = RequirementEntry {
            req_id: "REQ-AUDIT-002".to_string(),
            category: "AUDIT".to_string(),
            description: "Tamper detection on audit log".to_string(),
            status: "IN_PROGRESS".to_string(),
            priority: "MEDIUM".to_string(),
            dependencies: "REQ-AUDIT-001".to_string(),
            notes: "Hash chain verification".to_string(),
        };
        let preview = format_preview(&entry);
        assert!(preview.contains("REQ-AUDIT-002"));
        assert!(preview.contains("Status: IN_PROGRESS"));
        assert!(preview.contains("Dependencies: REQ-AUDIT-001"));
        assert!(preview.contains("Notes: Hash chain verification"));
    }
}
