//! Living documentation scanner and renderer.
//!
//! Parses Gherkin feature files and Rust step definitions into a `DocModel`,
//! then renders the model as Markdown or HTML.

use aegis_domain::error::DomainError;

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

/// A single step in a scenario (Given/When/Then/And/But).
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub keyword: String,
    pub text: String,
    pub line: usize,
}

/// A scenario parsed from a Gherkin feature file.
#[derive(Debug, Clone, PartialEq)]
pub struct Scenario {
    pub name: String,
    pub steps: Vec<Step>,
    pub tags: Vec<String>,
}

/// A feature parsed from a Gherkin feature file.
#[derive(Debug, Clone, PartialEq)]
pub struct Feature {
    pub name: String,
    pub description: String,
    pub file_path: String,
    pub scenarios: Vec<Scenario>,
}

/// A Rust step definition found via attribute scanning.
#[derive(Debug, Clone, PartialEq)]
pub struct StepDefinition {
    pub pattern: String,
    pub file_path: String,
    pub line: usize,
}

/// Aggregated documentation model produced by the scanner.
#[derive(Debug, Clone, PartialEq)]
pub struct DocModel {
    pub features: Vec<Feature>,
    pub step_definitions: Vec<StepDefinition>,
}

// ---------------------------------------------------------------------------
// Scanner -- feature files
// ---------------------------------------------------------------------------

/// Parse a Gherkin-like feature file into a [`Feature`].
///
/// Recognises `Feature:`, `Scenario:`, `Scenario Outline:`, step keywords
/// (`Given`, `When`, `Then`, `And`, `But`), and tags (lines starting with
/// `@`).
pub fn scan_feature_file(content: &str, file_path: &str) -> Result<Feature, DomainError> {
    let mut feature_name: Option<String> = None;
    let mut feature_desc = String::new();
    let mut scenarios: Vec<Scenario> = Vec::new();
    let mut pending_tags: Vec<String> = Vec::new();
    let mut current_scenario: Option<Scenario> = None;
    let mut in_description = false;

    for (idx, raw_line) in content.lines().enumerate() {
        let line_num = idx + 1;
        let trimmed = raw_line.trim();

        // Skip blank lines and comments (preserve description accumulation)
        if trimmed.is_empty() {
            in_description = false;
            continue;
        }
        if trimmed.starts_with('#') {
            continue;
        }

        // Tags
        if trimmed.starts_with('@') {
            in_description = false;
            for tag in trimmed.split_whitespace() {
                if tag.starts_with('@') {
                    pending_tags.push(tag.to_string());
                }
            }
            continue;
        }

        // Feature line
        if trimmed.starts_with("Feature:") {
            let name = trimmed
                .strip_prefix("Feature:")
                .unwrap_or("")
                .trim()
                .to_string();
            feature_name = Some(name);
            in_description = true;
            // Tags before Feature: are feature-level; ignore for now.
            pending_tags.clear();
            continue;
        }

        // Scenario / Scenario Outline
        if trimmed.starts_with("Scenario:") || trimmed.starts_with("Scenario Outline:") {
            in_description = false;
            // Flush previous scenario
            if let Some(s) = current_scenario.take() {
                scenarios.push(s);
            }
            let name = if let Some(rest) = trimmed.strip_prefix("Scenario Outline:") {
                rest.trim().to_string()
            } else {
                trimmed
                    .strip_prefix("Scenario:")
                    .unwrap_or("")
                    .trim()
                    .to_string()
            };
            current_scenario = Some(Scenario {
                name,
                steps: Vec::new(),
                tags: std::mem::take(&mut pending_tags),
            });
            continue;
        }

        // Steps
        let step_keywords = ["Given ", "When ", "Then ", "And ", "But "];
        let matched_kw = step_keywords.iter().find(|kw| trimmed.starts_with(*kw));
        if let Some(kw) = matched_kw {
            in_description = false;
            let keyword = kw.trim().to_string();
            let text = trimmed[kw.len()..].to_string();
            if let Some(ref mut s) = current_scenario {
                s.steps.push(Step {
                    keyword,
                    text,
                    line: line_num,
                });
            }
            continue;
        }

        // Description lines (between Feature: and first Scenario)
        if in_description && feature_name.is_some() {
            if !feature_desc.is_empty() {
                feature_desc.push('\n');
            }
            feature_desc.push_str(trimmed);
        }
    }

    // Flush last scenario
    if let Some(s) = current_scenario.take() {
        scenarios.push(s);
    }

    let name = feature_name
        .ok_or_else(|| DomainError::Other(format!("no Feature: line found in {file_path}")))?;

    Ok(Feature {
        name,
        description: feature_desc,
        file_path: file_path.to_string(),
        scenarios,
    })
}

// ---------------------------------------------------------------------------
// Scanner -- step definitions
// ---------------------------------------------------------------------------

/// Scan Rust source for step definition attributes.
///
/// Looks for patterns like `#[given(regex = r#"..."#)]`,
/// `#[when(regex = r#"..."#)]`, `#[then(regex = r#"..."#)]`.
pub fn scan_step_definitions(content: &str, file_path: &str) -> Vec<StepDefinition> {
    let mut defs = Vec::new();

    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        // Match #[given(regex = ...)] / #[when(regex = ...)] / #[then(regex = ...)]
        for attr in &["#[given(", "#[when(", "#[then("] {
            if let Some(rest) = trimmed.strip_prefix(attr)
                && let Some(pattern) = extract_regex_pattern(rest)
            {
                defs.push(StepDefinition {
                    pattern,
                    file_path: file_path.to_string(),
                    line: idx + 1,
                });
            }
        }
    }

    defs
}

/// Extract the regex string from the inside of an attribute like
/// `regex = r#"the agent ..."#)]`.
fn extract_regex_pattern(attr_body: &str) -> Option<String> {
    // Try regex = r#"..."#
    if let Some(after_eq) = attr_body.strip_prefix("regex = ") {
        return extract_raw_string(after_eq).or_else(|| extract_quoted_string(after_eq));
    }
    // Try regex = "..."
    if let Some(after_eq) = attr_body.strip_prefix("regex=") {
        return extract_raw_string(after_eq).or_else(|| extract_quoted_string(after_eq));
    }
    // Try bare string: #[given("...")]
    extract_raw_string(attr_body).or_else(|| extract_quoted_string(attr_body))
}

fn extract_raw_string(s: &str) -> Option<String> {
    // r#"..."#
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("r#\"") {
        let end = rest.find("\"#")?;
        return Some(rest[..end].to_string());
    }
    None
}

fn extract_quoted_string(s: &str) -> Option<String> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix('"') {
        // Find unescaped closing quote
        let mut chars = rest.chars();
        let mut result = String::new();
        while let Some(c) = chars.next() {
            if c == '\\' {
                if let Some(escaped) = chars.next() {
                    result.push(escaped);
                }
            } else if c == '"' {
                return Some(result);
            } else {
                result.push(c);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Renderers
// ---------------------------------------------------------------------------

/// Render a [`DocModel`] as Markdown.
pub fn render_markdown(model: &DocModel) -> String {
    let mut out = String::new();

    if model.features.is_empty() && model.step_definitions.is_empty() {
        out.push_str("# Living Documentation\n\nNo features found.\n");
        return out;
    }

    for feature in &model.features {
        out.push_str(&format!("# {}\n\n", feature.name));
        if !feature.description.is_empty() {
            out.push_str(&feature.description);
            out.push_str("\n\n");
        }

        for scenario in &feature.scenarios {
            // Tags as badges
            if !scenario.tags.is_empty() {
                let badges: Vec<String> =
                    scenario.tags.iter().map(|t| format!("`{t}`")).collect();
                out.push_str(&badges.join(" "));
                out.push('\n');
            }
            out.push_str(&format!("## {}\n\n", scenario.name));

            for (i, step) in scenario.steps.iter().enumerate() {
                out.push_str(&format!("{}. **{}** {}\n", i + 1, step.keyword, step.text));
            }
            out.push('\n');
        }
    }

    // Step definition cross-references
    if !model.step_definitions.is_empty() {
        out.push_str("---\n\n## Step Definitions\n\n");
        for sd in &model.step_definitions {
            out.push_str(&format!(
                "- `{}` ({} line {})\n",
                sd.pattern, sd.file_path, sd.line
            ));
        }
    }

    out
}

/// Render a [`DocModel`] as a self-contained HTML page.
pub fn render_html(model: &DocModel) -> String {
    let md = render_markdown(model);
    // Minimal HTML with inline CSS; no external dependency needed.
    let mut html = String::new();
    html.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n");
    html.push_str("<meta charset=\"utf-8\">\n");
    html.push_str("<title>Living Documentation</title>\n");
    html.push_str("<style>\n");
    html.push_str("body { font-family: sans-serif; max-width: 48em; ");
    html.push_str("margin: 2em auto; padding: 0 1em; line-height: 1.6; }\n");
    html.push_str("h1 { border-bottom: 2px solid #333; }\n");
    html.push_str("h2 { color: #444; }\n");
    html.push_str("code { background: #f4f4f4; padding: 0.1em 0.3em; ");
    html.push_str("border-radius: 3px; }\n");
    html.push_str("ol { padding-left: 1.5em; }\n");
    html.push_str("</style>\n</head>\n<body>\n<pre>\n");
    html.push_str(&html_escape(&md));
    html.push_str("</pre>\n</body>\n</html>\n");
    html
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_FEATURE: &str = "\
@wip
Feature: User Authentication
  As a defense engineer
  I need secure authentication

  @smoke @critical
  Scenario: Successful login
    Given a valid user credential
    When the user submits the login form
    Then the user should be authenticated
    And a session token should be issued

  Scenario: Failed login
    Given an invalid user credential
    When the user submits the login form
    Then the user should see an error message
";

    // rtmx:req REQ-TEST-056
    #[test]
    fn test_scanner_produces_doc_model() {
        let feature = scan_feature_file(SAMPLE_FEATURE, "tests/features/auth.feature")
            .expect("should parse");

        assert_eq!(feature.name, "User Authentication");
        assert_eq!(feature.file_path, "tests/features/auth.feature");
        assert_eq!(feature.scenarios.len(), 2);

        let s0 = &feature.scenarios[0];
        assert_eq!(s0.name, "Successful login");
        assert_eq!(s0.tags, vec!["@smoke".to_string(), "@critical".to_string()]);
        assert_eq!(s0.steps.len(), 4);
        assert_eq!(s0.steps[0].keyword, "Given");
        assert_eq!(s0.steps[0].text, "a valid user credential");
        assert_eq!(s0.steps[3].keyword, "And");

        let s1 = &feature.scenarios[1];
        assert_eq!(s1.name, "Failed login");
        assert!(s1.tags.is_empty());
        assert_eq!(s1.steps.len(), 3);
    }

    // rtmx:req REQ-TEST-056
    #[test]
    fn test_parse_tags() {
        let input = "\
Feature: Tagged
  @alpha @beta
  Scenario: One
    Given something
  @gamma
  Scenario: Two
    When something else
";
        let feature = scan_feature_file(input, "f.feature").unwrap();
        assert_eq!(feature.scenarios[0].tags, vec!["@alpha", "@beta"]);
        assert_eq!(feature.scenarios[1].tags, vec!["@gamma"]);
    }

    // rtmx:req REQ-TEST-056
    #[test]
    fn test_parse_scenario_outline() {
        let input = "\
Feature: Outlines
  Scenario Outline: Parameterised test
    Given a value <val>
    When it is processed
    Then the result is <result>
";
        let feature = scan_feature_file(input, "o.feature").unwrap();
        assert_eq!(feature.scenarios[0].name, "Parameterised test");
        assert_eq!(feature.scenarios[0].steps.len(), 3);
    }

    // rtmx:req REQ-TEST-056
    #[test]
    fn test_parse_step_definitions_from_rust() {
        let rust_src = r##"
use cucumber::{given, when, then};

#[given(regex = r#"the agent is configured with a valid LLM provider"#)]
fn step_agent_configured(world: &mut TestWorld) {}

#[when(regex = r#"the user sends "([^"]+)""#)]
fn step_user_sends(world: &mut TestWorld, msg: String) {}

#[then(regex = r#"the agent should invoke "([^"]+)" on "([^"]+)""#)]
fn step_invoke(world: &mut TestWorld, tool: String, target: String) {}
"##;
        let defs = scan_step_definitions(rust_src, "tests/steps/agent.rs");
        assert_eq!(defs.len(), 3);
        assert_eq!(
            defs[0].pattern,
            "the agent is configured with a valid LLM provider"
        );
        assert_eq!(defs[0].file_path, "tests/steps/agent.rs");
        assert_eq!(defs[1].pattern, r#"the user sends "([^"]+)""#);
        assert_eq!(
            defs[2].pattern,
            r#"the agent should invoke "([^"]+)" on "([^"]+)""#
        );
    }

    // rtmx:req REQ-TEST-056
    #[test]
    fn test_empty_input_returns_error() {
        let result = scan_feature_file("", "empty.feature");
        assert!(result.is_err());
    }

    // rtmx:req REQ-TEST-056
    #[test]
    fn test_malformed_input_no_feature_line() {
        let result = scan_feature_file("Scenario: orphan\n  Given x\n", "bad.feature");
        assert!(result.is_err());
    }

    // rtmx:req REQ-TEST-056
    #[test]
    fn test_scan_step_definitions_empty_input() {
        let defs = scan_step_definitions("", "empty.rs");
        assert!(defs.is_empty());
    }

    // rtmx:req REQ-TEST-057
    #[test]
    fn test_renderer_produces_html() {
        let model = DocModel {
            features: vec![Feature {
                name: "Auth".to_string(),
                description: "Auth description".to_string(),
                file_path: "auth.feature".to_string(),
                scenarios: vec![Scenario {
                    name: "Login".to_string(),
                    steps: vec![Step {
                        keyword: "Given".to_string(),
                        text: "a user".to_string(),
                        line: 3,
                    }],
                    tags: vec!["@smoke".to_string()],
                }],
            }],
            step_definitions: vec![StepDefinition {
                pattern: "a user".to_string(),
                file_path: "steps.rs".to_string(),
                line: 10,
            }],
        };

        let html = render_html(&model);
        assert!(html.contains("<html"));
        assert!(html.contains("</html>"));
        assert!(html.contains("<body>"));
        assert!(html.contains("</body>"));
        assert!(html.contains("Auth"));
        assert!(html.contains("Login"));
    }

    // rtmx:req REQ-TEST-057
    #[test]
    fn test_markdown_output_contains_headers() {
        let model = DocModel {
            features: vec![Feature {
                name: "Infra Setup".to_string(),
                description: String::new(),
                file_path: "infra.feature".to_string(),
                scenarios: vec![Scenario {
                    name: "Provision".to_string(),
                    steps: vec![
                        Step {
                            keyword: "Given".to_string(),
                            text: "a cloud account".to_string(),
                            line: 2,
                        },
                        Step {
                            keyword: "When".to_string(),
                            text: "I run provision".to_string(),
                            line: 3,
                        },
                    ],
                    tags: vec!["@infra".to_string()],
                }],
            }],
            step_definitions: vec![],
        };

        let md = render_markdown(&model);
        assert!(md.contains("# Infra Setup"));
        assert!(md.contains("## Provision"));
        assert!(md.contains("**Given**"));
        assert!(md.contains("**When**"));
        assert!(md.contains("`@infra`"));
    }

    // rtmx:req REQ-TEST-057
    #[test]
    fn test_empty_model_produces_valid_output() {
        let model = DocModel {
            features: vec![],
            step_definitions: vec![],
        };

        let md = render_markdown(&model);
        assert!(md.contains("Living Documentation"));
        assert!(md.contains("No features found"));

        let html = render_html(&model);
        assert!(html.contains("<html"));
        assert!(html.contains("</html>"));
        assert!(html.contains("No features found"));
    }
}
