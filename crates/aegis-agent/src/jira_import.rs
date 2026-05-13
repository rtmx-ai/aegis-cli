//! JIRA CSV import with deduplication and merge-or-skip logic.
//!
//! Parses JIRA CSV exports into `JiraIssue` structs and imports them into
//! an existing RTM database, deduplicating by external_id stored in the
//! requirement's `notes` field.

use aegis_domain::DomainError;
use aegis_domain::rtmx::RequirementsDb;

/// A parsed JIRA issue from CSV export.
#[derive(Debug, Clone)]
pub struct JiraIssue {
    pub key: String,
    pub summary: String,
    pub priority: String,
    pub status: String,
    pub description: String,
    pub issue_type: String,
}

/// A row ready for import into the RTM database.
#[derive(Debug, Clone)]
pub struct RtmImportRow {
    pub req_id: String,
    pub category: String,
    pub requirement_text: String,
    pub priority: String,
    pub status: String,
    pub external_id: String,
}

/// Result of importing JIRA issues into the RTM.
#[derive(Debug, Clone)]
pub struct ImportResult {
    /// New rows to add to the RTM.
    pub added: Vec<RtmImportRow>,
    /// Issues skipped (exact duplicates). Each string explains why.
    pub skipped: Vec<String>,
    /// Issues merged (updated text). Each string is the req_id that was updated.
    pub merged: Vec<String>,
}

// ---------------------------------------------------------------------------
// REQ-RTMX-030: CSV parser with JIRA field mapping
// ---------------------------------------------------------------------------

/// Parse a JIRA CSV export into a list of `JiraIssue` structs.
///
/// Handles RFC 4180-compatible CSV: quoted fields may contain commas and
/// escaped double quotes (`""`). Column headers are matched case-insensitively.
pub fn parse_jira_csv(csv_content: &str) -> Result<Vec<JiraIssue>, DomainError> {
    let rows = parse_csv_rows(csv_content);
    if rows.is_empty() {
        return Err(DomainError::Other(
            "Malformed CSV: no header row found".to_string(),
        ));
    }

    let headers = &rows[0];
    if headers.is_empty() || (headers.len() == 1 && headers[0].is_empty()) {
        return Err(DomainError::Other(
            "Malformed CSV: no header row found".to_string(),
        ));
    }

    // Build a case-insensitive column index.
    let col_index = |name: &str| -> Option<usize> {
        let lower = name.to_ascii_lowercase();
        headers
            .iter()
            .position(|h| h.trim().to_ascii_lowercase() == lower)
    };

    let mut issues = Vec::new();

    for row in rows.iter().skip(1) {
        if row.is_empty() || (row.len() == 1 && row[0].is_empty()) {
            continue;
        }

        let get = |name: &str| -> String {
            col_index(name)
                .and_then(|i| row.get(i))
                .cloned()
                .unwrap_or_default()
        };

        let key = get("Key");
        if key.is_empty() {
            continue;
        }

        issues.push(JiraIssue {
            key,
            summary: get("Summary"),
            priority: get("Priority"),
            status: get("Status"),
            description: get("Description"),
            issue_type: get("Issue Type"),
        });
    }

    Ok(issues)
}

// ---------------------------------------------------------------------------
// REQ-RTMX-031: Deduplication and merge-or-skip on existing req_ids
// ---------------------------------------------------------------------------

/// Import JIRA issues into an RTM database, deduplicating against existing
/// entries.
///
/// Deduplication uses the requirement's `notes` field to store the JIRA key
/// as an external reference. For each issue:
///
/// - **Skip** if an existing requirement's notes contain the JIRA key *and*
///   the requirement text matches the issue summary.
/// - **Merge** if an existing requirement's notes contain the JIRA key but
///   the requirement text differs (the text is updated, status is kept).
/// - **Add** if no existing requirement references this JIRA key.
///
/// Generated req_ids use the format `REQ-JIRA-{KEY}` (uppercase).
pub fn import_jira_issues(issues: &[JiraIssue], existing: &RequirementsDb) -> ImportResult {
    let mut result = ImportResult {
        added: Vec::new(),
        skipped: Vec::new(),
        merged: Vec::new(),
    };

    for issue in issues {
        let external_id = issue.key.to_uppercase();
        let req_id = format!("REQ-JIRA-{}", external_id);

        // Search existing requirements for one whose notes contain this key.
        let existing_match = existing
            .all()
            .iter()
            .find(|r| r.notes.contains(&external_id));

        match existing_match {
            Some(existing_req) => {
                if existing_req.requirement_text == issue.summary {
                    // Exact duplicate -- skip.
                    result.skipped.push(format!(
                        "Skipped {}: exact duplicate of {}",
                        external_id, existing_req.req_id
                    ));
                } else {
                    // Same external_id, different text -- merge (update text).
                    result.merged.push(existing_req.req_id.clone());
                }
            }
            None => {
                // New entry.
                result.added.push(RtmImportRow {
                    req_id,
                    category: "JIRA".to_string(),
                    requirement_text: issue.summary.clone(),
                    priority: issue.priority.clone(),
                    status: "MISSING".to_string(),
                    external_id,
                });
            }
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Simple RFC 4180 CSV parser
// ---------------------------------------------------------------------------

/// Parse CSV content into a vector of rows, each row being a vector of field
/// strings. Handles quoted fields containing commas, newlines, and escaped
/// double-quotes (`""`).
fn parse_csv_rows(input: &str) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut current_row: Vec<String> = Vec::new();
    let mut current_field = String::new();
    let mut in_quotes = false;
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    // Escaped double-quote.
                    chars.next();
                    current_field.push('"');
                } else {
                    // End of quoted field.
                    in_quotes = false;
                }
            } else {
                current_field.push(ch);
            }
        } else {
            match ch {
                '"' => {
                    in_quotes = true;
                }
                ',' => {
                    current_row.push(current_field.clone());
                    current_field.clear();
                }
                '\n' => {
                    current_row.push(current_field.clone());
                    current_field.clear();
                    rows.push(current_row.clone());
                    current_row.clear();
                }
                '\r' => {
                    // Skip \r; the following \n (if any) will end the row.
                    if chars.peek() != Some(&'\n') {
                        current_row.push(current_field.clone());
                        current_field.clear();
                        rows.push(current_row.clone());
                        current_row.clear();
                    }
                }
                _ => {
                    current_field.push(ch);
                }
            }
        }
    }

    // Flush the last field/row if there is remaining content.
    if !current_field.is_empty() || !current_row.is_empty() {
        current_row.push(current_field);
        rows.push(current_row);
    }

    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-RTMX-030
    #[test]
    fn test_jira_csv_parser() {
        let csv = "\
Summary,Key,Priority,Status,Description,Issue Type
Implement auth flow,AUTH-1,High,Open,Auth description,Story
Fix login bug,AUTH-2,Medium,In Progress,Login fix,Bug";

        let issues = parse_jira_csv(csv).unwrap();
        assert_eq!(issues.len(), 2);

        assert_eq!(issues[0].key, "AUTH-1");
        assert_eq!(issues[0].summary, "Implement auth flow");
        assert_eq!(issues[0].priority, "High");
        assert_eq!(issues[0].status, "Open");
        assert_eq!(issues[0].description, "Auth description");
        assert_eq!(issues[0].issue_type, "Story");

        assert_eq!(issues[1].key, "AUTH-2");
        assert_eq!(issues[1].summary, "Fix login bug");
        assert_eq!(issues[1].priority, "Medium");
        assert_eq!(issues[1].status, "In Progress");
        assert_eq!(issues[1].description, "Login fix");
        assert_eq!(issues[1].issue_type, "Bug");
    }

    // rtmx:req REQ-RTMX-030
    #[test]
    fn test_jira_csv_parser_quoted_fields() {
        let csv = "\
Summary,Key,Priority,Status,Description,Issue Type
\"Summary with, comma\",PROJ-10,High,Open,\"Description with, comma and \"\"quotes\"\"\",Story";

        let issues = parse_jira_csv(csv).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].summary, "Summary with, comma");
        assert_eq!(
            issues[0].description,
            "Description with, comma and \"quotes\""
        );
        assert_eq!(issues[0].key, "PROJ-10");
    }

    // rtmx:req REQ-RTMX-030
    #[test]
    fn test_jira_csv_parser_missing_columns() {
        // CSV with only Key and Summary -- other columns should default to empty.
        let csv = "\
Key,Summary
PROJ-1,Just a summary";

        let issues = parse_jira_csv(csv).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].key, "PROJ-1");
        assert_eq!(issues[0].summary, "Just a summary");
        assert_eq!(issues[0].priority, "");
        assert_eq!(issues[0].status, "");
        assert_eq!(issues[0].description, "");
        assert_eq!(issues[0].issue_type, "");
    }

    // rtmx:req REQ-RTMX-030
    #[test]
    fn test_jira_csv_parser_empty_input() {
        let csv = "Summary,Key,Priority,Status,Description,Issue Type\n";
        let issues = parse_jira_csv(csv).unwrap();
        assert!(issues.is_empty());
    }

    // rtmx:req REQ-RTMX-030
    #[test]
    fn test_jira_csv_parser_invalid_input() {
        // Completely empty string -- no headers at all.
        let result = parse_jira_csv("");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("header"),
            "Error should mention missing headers, got: {err}"
        );
    }

    // Helper: build a RequirementsDb with specific notes fields for dedup testing.
    fn make_db(entries: &[(&str, &str, &str)]) -> RequirementsDb {
        // entries: (req_id, requirement_text, notes)
        let mut csv = String::from(
            "req_id,category,subcategory,requirement_text,target_value,\
             test_module,test_function,validation_method,status,priority,\
             phase,notes,effort_weeks,dependencies,blocks,assignee,\
             sprint,started_date,completed_date\n",
        );
        for (req_id, text, notes) in entries {
            csv.push_str(&format!(
                "{},JIRA,IMPORT,{},,,,,MISSING,HIGH,1,{},,,,,,,\n",
                req_id, text, notes
            ));
        }
        RequirementsDb::from_csv(&csv).unwrap()
    }

    // rtmx:req REQ-RTMX-031
    #[test]
    fn test_jira_import_deduplicates() {
        let db = make_db(&[("REQ-JIRA-PROJ-1", "Existing summary", "PROJ-1")]);

        let issues = vec![JiraIssue {
            key: "PROJ-1".to_string(),
            summary: "Existing summary".to_string(),
            priority: "High".to_string(),
            status: "Open".to_string(),
            description: "desc".to_string(),
            issue_type: "Story".to_string(),
        }];

        let result = import_jira_issues(&issues, &db);
        assert!(result.added.is_empty(), "Should not add exact duplicate");
        assert_eq!(result.skipped.len(), 1);
        assert!(result.merged.is_empty());
        assert!(
            result.skipped[0].contains("PROJ-1"),
            "Skip reason should mention the key"
        );
    }

    // rtmx:req REQ-RTMX-031
    #[test]
    fn test_jira_import_merges_updates() {
        let db = make_db(&[("REQ-JIRA-PROJ-2", "Old summary text", "PROJ-2")]);

        let issues = vec![JiraIssue {
            key: "PROJ-2".to_string(),
            summary: "Updated summary text".to_string(),
            priority: "High".to_string(),
            status: "Open".to_string(),
            description: "desc".to_string(),
            issue_type: "Story".to_string(),
        }];

        let result = import_jira_issues(&issues, &db);
        assert!(result.added.is_empty());
        assert!(result.skipped.is_empty());
        assert_eq!(result.merged.len(), 1);
        assert_eq!(result.merged[0], "REQ-JIRA-PROJ-2");
    }

    // rtmx:req REQ-RTMX-031
    #[test]
    fn test_jira_import_adds_new() {
        let db = make_db(&[]);

        let issues = vec![JiraIssue {
            key: "NEW-1".to_string(),
            summary: "Brand new issue".to_string(),
            priority: "Medium".to_string(),
            status: "Open".to_string(),
            description: "new desc".to_string(),
            issue_type: "Task".to_string(),
        }];

        let result = import_jira_issues(&issues, &db);
        assert_eq!(result.added.len(), 1);
        assert!(result.skipped.is_empty());
        assert!(result.merged.is_empty());

        let row = &result.added[0];
        assert_eq!(row.req_id, "REQ-JIRA-NEW-1");
        assert_eq!(row.category, "JIRA");
        assert_eq!(row.requirement_text, "Brand new issue");
        assert_eq!(row.priority, "Medium");
        assert_eq!(row.status, "MISSING");
        assert_eq!(row.external_id, "NEW-1");
    }

    // rtmx:req REQ-RTMX-015
    // rtmx:req REQ-RTMX-010
    #[test]
    fn test_jira_csv_import_end_to_end() {
        // Verify full pipeline: parse JIRA CSV -> merge into RTM.
        let csv = "Key,Summary,Priority,Status,Description,Issue Type\n\
                   PROJ-42,Fix auth bug,High,In Progress,Auth is broken,Bug\n";
        let issues = parse_jira_csv(csv).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].key, "PROJ-42");

        let db = make_db(&[]);
        let result = import_jira_issues(&issues, &db);
        assert_eq!(result.added.len(), 1);
        assert!(
            result.added[0].req_id.starts_with("REQ-JIRA-"),
            "JIRA imports must use REQ-JIRA- prefix"
        );
    }
}
