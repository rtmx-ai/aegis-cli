//! Slash command palette: floating autocomplete dropdown.
//!
//! Shows available commands when the user types `/` in the input field.
//! Supports prefix filtering, up/down navigation, and Tab completion.

use std::collections::HashMap;

/// A slash command entry for the palette.
#[derive(Debug, Clone)]
pub struct CommandEntry {
    pub name: String,
    pub description: String,
    pub usage: Option<String>,
}

/// Snapshot of palette state for rendering.
#[derive(Debug, Clone)]
pub struct CommandPaletteView {
    pub entries: Vec<CommandEntry>,
    pub selected: usize,
    /// Stage hint for the dropdown title (e.g., "Select provider:").
    pub stage_hint: Option<String>,
}

/// The command palette state.
pub struct CommandPalette {
    all_commands: Vec<CommandEntry>,
    pub(crate) filtered: Vec<CommandEntry>,
    pub(crate) selected: usize,
    pub is_visible: bool,
    /// Current stage: command selection or token-level argument selection.
    pub stage: PaletteStage,
    /// Dynamically-injected options keyed by slot name (e.g., "project").
    /// Populated asynchronously by CSP discovery; consumed by build_slot_entries().
    injected_options: HashMap<String, Vec<TokenOption>>,
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandPalette {
    pub fn new() -> Self {
        let all_commands = vec![
            cmd("/help", "Show available commands and usage"),
            cmd_with_usage(
                "/connect",
                "Connect to an LLM provider",
                "<local|vertex|bedrock|azure>",
            ),
            cmd_with_usage("/model", "Switch or display current model", "<name>"),
            cmd_with_usage("/add", "Add file to conversation context", "<path>"),
            cmd_with_usage("/drop", "Remove file from context", "<path>"),
            cmd("/context", "Show current context files"),
            cmd_with_usage("/search", "Search conversation history", "<query>"),
            cmd_with_usage(
                "/infra",
                "Infrastructure plugin operations",
                "<list|status|up|preview|destroy>",
            ),
            cmd("/doctor", "Run health and connectivity checks"),
            cmd("/copy", "Copy last code block to clipboard"),
            cmd("/undo", "Revert most recent approved write"),
            cmd("/clear", "Clear conversation history"),
            cmd("/quit", "Exit aegis"),
        ];
        Self {
            filtered: all_commands.clone(),
            all_commands,
            selected: 0,
            is_visible: false,
            stage: PaletteStage::CommandSelection,
            injected_options: HashMap::new(),
        }
    }

    pub fn show(&mut self) {
        self.is_visible = true;
        self.selected = 0;
        self.filtered = self.all_commands.clone();
        self.stage = PaletteStage::CommandSelection;
    }

    pub fn hide(&mut self) {
        self.is_visible = false;
        self.stage = PaletteStage::CommandSelection;
    }

    pub fn filter(&mut self, prefix: &str) {
        let p = prefix.to_lowercase();
        self.filtered = self
            .all_commands
            .iter()
            .filter(|c| c.name.to_lowercase().starts_with(&p))
            .cloned()
            .collect();
        self.selected = 0;
    }

    pub fn next(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = (self.selected + 1) % self.filtered.len();
        }
    }

    pub fn prev(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = self
                .selected
                .checked_sub(1)
                .unwrap_or(self.filtered.len() - 1);
        }
    }

    pub fn selected_entry(&self) -> Option<&CommandEntry> {
        self.filtered.get(self.selected)
    }

    pub fn selected_command(&self) -> Option<&str> {
        self.filtered.get(self.selected).map(|e| e.name.as_str())
    }

    pub fn view(&self) -> Option<CommandPaletteView> {
        if !self.is_visible || self.filtered.is_empty() {
            return None;
        }
        Some(CommandPaletteView {
            entries: self.filtered.clone(),
            selected: self.selected,
            stage_hint: self.stage_hint(),
        })
    }

    /// Inject dynamically-discovered options for a named slot.
    /// Called when async CSP discovery completes.
    pub fn inject_options(&mut self, slot_name: &str, options: Vec<TokenOption>) {
        self.injected_options.insert(slot_name.to_string(), options);
    }

    /// Clear injected options for a named slot.
    pub fn clear_injected(&mut self, slot_name: &str) {
        self.injected_options.remove(slot_name);
    }

    /// Re-run build_slot_entries for the current slot to refresh the filtered list.
    /// Called when injected options arrive mid-palette-session.
    pub fn refresh_current_slot(&mut self) {
        let (slot, selected_values) = match &self.stage {
            PaletteStage::TokenSelection {
                grammar,
                slot_index,
                selected_values,
            } => match grammar.slots.get(*slot_index) {
                Some(s) => (s.clone(), selected_values.clone()),
                None => return,
            },
            _ => return,
        };
        let options = self.build_slot_entries(&slot, &selected_values);
        self.filtered = options;
        if self.selected >= self.filtered.len() {
            self.selected = 0;
        }
    }
}

// ---------------------------------------------------------------------------
// REQ-TUI-063a: Structured token-level command grammar
// ---------------------------------------------------------------------------

/// Kind of value a token slot accepts.
#[derive(Debug, Clone)]
pub enum TokenKind {
    /// Fixed set of options shown as a dropdown.
    Enum(Vec<TokenOption>),
    /// Free-text with a placeholder hint.
    FreeText { placeholder: String },
}

/// A single option in an Enum-kind token slot.
#[derive(Debug, Clone)]
pub struct TokenOption {
    pub value: String,
    pub label: String,
    pub description: String,
}

/// One argument position in a command's grammar.
#[derive(Debug, Clone)]
pub struct TokenSlot {
    pub name: String,
    pub kind: TokenKind,
    pub required: bool,
    /// Flag prefix for named args (e.g., "--model="). None for positional.
    pub prefix: Option<String>,
}

/// Grammar for a slash command: the command name + its argument slots.
#[derive(Debug, Clone)]
pub struct CommandGrammar {
    pub name: String,
    pub description: String,
    pub slots: Vec<TokenSlot>,
}

/// Tracks which stage of the palette the user is in.
#[derive(Debug, Clone)]
pub enum PaletteStage {
    /// Selecting the command name (existing behavior).
    CommandSelection,
    /// Selecting a token value at a specific slot index.
    TokenSelection {
        grammar: CommandGrammar,
        slot_index: usize,
        /// Values already selected for previous slots.
        selected_values: Vec<String>,
    },
}

impl CommandPalette {
    /// Return the grammar for a command, if it has structured arguments.
    pub fn grammar_for(&self, command_name: &str) -> Option<CommandGrammar> {
        match command_name {
            "/connect" => Some(connect_grammar()),
            _ => None,
        }
    }

    /// Transition to token selection for a command with a grammar.
    /// Populates the filtered list with the first slot's options.
    pub fn enter_token_stage(&mut self, grammar: CommandGrammar) {
        let options = self.build_slot_entries(&grammar.slots[0], &[]);
        self.filtered = options;
        self.selected = 0;
        self.stage = PaletteStage::TokenSelection {
            grammar,
            slot_index: 0,
            selected_values: Vec::new(),
        };
    }

    /// Advance to the next token slot after a selection. Returns false
    /// if there are no more slots (palette should hide).
    pub fn advance_token(&mut self, selected_value: String) -> bool {
        let (grammar, slot_index, mut values) = match self.stage.clone() {
            PaletteStage::TokenSelection {
                grammar,
                slot_index,
                selected_values,
            } => (grammar, slot_index, selected_values),
            _ => return false,
        };
        values.push(selected_value);
        let next_index = slot_index + 1;
        if next_index >= grammar.slots.len() {
            self.hide();
            return false;
        }
        let options = self.build_slot_entries(&grammar.slots[next_index], &values);
        self.filtered = options;
        self.selected = 0;
        self.stage = PaletteStage::TokenSelection {
            grammar,
            slot_index: next_index,
            selected_values: values,
        };
        true
    }

    /// Go back to the previous token slot. Returns false if already at
    /// command selection stage.
    pub fn retreat_token(&mut self) -> bool {
        let (grammar, slot_index, mut values) = match self.stage.clone() {
            PaletteStage::TokenSelection {
                grammar,
                slot_index,
                selected_values,
            } => (grammar, slot_index, selected_values),
            _ => return false,
        };
        if slot_index == 0 {
            // Go back to command selection
            self.stage = PaletteStage::CommandSelection;
            self.show();
            return false;
        }
        values.pop();
        let prev_index = slot_index - 1;
        let options = self.build_slot_entries(&grammar.slots[prev_index], &values);
        self.filtered = options;
        self.selected = 0;
        self.stage = PaletteStage::TokenSelection {
            grammar,
            slot_index: prev_index,
            selected_values: values,
        };
        true
    }

    /// Filter current token slot options by prefix.
    pub fn filter_token(&mut self, prefix: &str) {
        let (grammar, slot_index, values) = match &self.stage {
            PaletteStage::TokenSelection {
                grammar,
                slot_index,
                selected_values,
            } => (grammar.clone(), *slot_index, selected_values.clone()),
            _ => return,
        };
        let all = self.build_slot_entries(&grammar.slots[slot_index], &values);
        let p = prefix.to_lowercase();
        self.filtered = all
            .into_iter()
            .filter(|e| {
                e.name.to_lowercase().contains(&p) || e.description.to_lowercase().contains(&p)
            })
            .collect();
        self.selected = 0;
    }

    /// Current stage hint for rendering (e.g., "Select provider:").
    pub fn stage_hint(&self) -> Option<String> {
        match &self.stage {
            PaletteStage::CommandSelection => None,
            PaletteStage::TokenSelection {
                grammar,
                slot_index,
                ..
            } => grammar
                .slots
                .get(*slot_index)
                .map(|s| format!("Select {}:", s.name)),
        }
    }

    /// Whether we are in token selection (not command selection).
    pub fn in_token_stage(&self) -> bool {
        matches!(self.stage, PaletteStage::TokenSelection { .. })
    }
}

impl CommandPalette {
    /// Build palette entries for a token slot, checking injected options first,
    /// then falling back to hardcoded options_for_provider().
    fn build_slot_entries(
        &self,
        slot: &TokenSlot,
        selected_values: &[String],
    ) -> Vec<CommandEntry> {
        match &slot.kind {
            TokenKind::Enum(options) => {
                let effective = if options.is_empty() {
                    // Check injected options first
                    if let Some(injected) = self.injected_options.get(&slot.name) {
                        if !injected.is_empty() {
                            injected.clone()
                        } else {
                            // Injected but empty -- fall back to hardcoded
                            let provider =
                                selected_values.first().map(|s| s.as_str()).unwrap_or("");
                            let hardcoded = options_for_provider(provider, &slot.name);
                            if hardcoded.is_empty() {
                                // No hardcoded either -- show loading indicator
                                return vec![CommandEntry {
                                    name: "Discovering projects...".into(),
                                    description: "Querying CSP for available projects".into(),
                                    usage: None,
                                }];
                            }
                            hardcoded
                        }
                    } else {
                        // No injected options -- use hardcoded
                        let provider = selected_values.first().map(|s| s.as_str()).unwrap_or("");
                        options_for_provider(provider, &slot.name)
                    }
                } else {
                    options.clone()
                };
                effective
                    .iter()
                    .map(|o| CommandEntry {
                        name: o.label.clone(),
                        description: o.description.clone(),
                        usage: None,
                    })
                    .collect()
            }
            TokenKind::FreeText { placeholder } => {
                vec![CommandEntry {
                    name: placeholder.clone(),
                    description: format!("Type a value for {}", slot.name),
                    usage: None,
                }]
            }
        }
    }
}

fn opt(value: &str, label: &str, desc: &str) -> TokenOption {
    TokenOption {
        value: value.into(),
        label: label.into(),
        description: desc.into(),
    }
}

/// Build the /connect command grammar with provider, model, region, project slots.
pub fn connect_grammar() -> CommandGrammar {
    CommandGrammar {
        name: "/connect".into(),
        description: "Connect to an LLM provider".into(),
        slots: vec![
            TokenSlot {
                name: "provider".into(),
                kind: TokenKind::Enum(vec![
                    opt("vertex", "vertex", "Google Vertex AI (Gemini)"),
                    opt("bedrock", "bedrock", "AWS Bedrock (Claude)"),
                    opt("azure", "azure", "Azure OpenAI (GPT)"),
                    opt("local", "local", "Ollama / vLLM (localhost)"),
                ]),
                required: true,
                prefix: None,
            },
            TokenSlot {
                name: "model".into(),
                kind: TokenKind::Enum(vec![]), // populated dynamically per provider
                required: false,
                prefix: Some("--model=".into()),
            },
            TokenSlot {
                name: "region".into(),
                kind: TokenKind::Enum(vec![]), // populated dynamically per provider
                required: false,
                prefix: Some("--region=".into()),
            },
            TokenSlot {
                name: "project".into(),
                kind: TokenKind::Enum(vec![]), // populated dynamically by CSP discovery
                required: false,
                prefix: Some("--project=".into()),
            },
        ],
    }
}

/// Populate model/region options dynamically based on provider selection.
/// Model lists sourced from gov cloud documentation as of April 2026.
pub fn options_for_provider(provider: &str, slot_name: &str) -> Vec<TokenOption> {
    match (provider, slot_name) {
        // GCP Vertex AI: IL4/IL5 (Gemini) + FedRAMP High (Claude)
        ("vertex", "model") => vec![
            opt(
                "gemini-3.1-pro",
                "Gemini 3.1 Pro",
                "Latest flagship, IL4/IL5",
            ),
            opt(
                "gemini-3-flash",
                "Gemini 3 Flash",
                "Agentic/reasoning, IL4/IL5",
            ),
            opt(
                "gemini-3.1-flash-lite",
                "Gemini 3.1 Flash Lite",
                "Cost-efficient, IL4/IL5",
            ),
            opt(
                "claude-opus-4.7",
                "Claude Opus 4.7",
                "Latest Claude, FedRAMP High only",
            ),
            opt(
                "claude-sonnet-4.6",
                "Claude Sonnet 4.6",
                "1M context, FedRAMP High only",
            ),
        ],
        ("vertex", "region") => vec![
            opt("us-central1", "US Central", "Iowa (Assured Workloads)"),
            opt("us-east4", "US East", "Virginia (Assured Workloads)"),
        ],
        // AWS GovCloud Bedrock: IL4/IL5
        ("bedrock", "model") => vec![
            opt(
                "claude-opus-sonnet-4.5",
                "Claude Sonnet 4.5",
                "Latest, IL4/IL5",
            ),
            opt(
                "claude-haiku-3",
                "Claude 3 Haiku",
                "Fast, cost-efficient, IL4/IL5",
            ),
        ],
        ("bedrock", "region") => vec![opt("us-gov-west-1", "US Gov West", "GovCloud (IL4/IL5)")],
        // Azure Government: FedRAMP High
        ("azure", "model") => vec![
            opt("gpt-5.1", "GPT-5.1", "Latest flagship, FedRAMP High"),
            opt("gpt-4.1", "GPT-4.1", "General purpose, FedRAMP High"),
            opt(
                "gpt-4.1-mini",
                "GPT-4.1 Mini",
                "Cost-efficient, FedRAMP High",
            ),
            opt("o3-mini", "o3-mini", "Reasoning model, FedRAMP High"),
        ],
        ("azure", "region") => vec![
            opt("usgovvirginia", "US Gov Virginia", "Azure Government"),
            opt("usgovarizona", "US Gov Arizona", "Azure Government"),
        ],
        // Local: common Ollama models
        ("local", "model") => vec![
            opt("llama3", "Llama 3", "Meta, 8B, general purpose"),
            opt("codellama", "Code Llama", "Meta, code-focused"),
            opt("mistral", "Mistral", "7B, fast inference"),
        ],
        _ => vec![],
    }
}

fn cmd(name: &str, desc: &str) -> CommandEntry {
    CommandEntry {
        name: name.into(),
        description: desc.into(),
        usage: None,
    }
}

fn cmd_with_usage(name: &str, desc: &str, usage: &str) -> CommandEntry {
    CommandEntry {
        name: name.into(),
        description: desc.into(),
        usage: Some(usage.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-TUI-040
    #[test]
    fn new_has_all_commands() {
        let p = CommandPalette::new();
        assert!(p.all_commands.len() >= 11);
    }

    // rtmx:req REQ-TUI-040
    #[test]
    fn filter_by_prefix() {
        let mut p = CommandPalette::new();
        p.filter("/c");
        let names: Vec<_> = p.filtered.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"/connect"));
        assert!(names.contains(&"/context"));
        assert!(names.contains(&"/clear"));
        assert!(!names.contains(&"/help"));
    }

    // rtmx:req REQ-TUI-040
    #[test]
    fn filter_no_match() {
        let mut p = CommandPalette::new();
        p.filter("/xyz");
        assert!(p.filtered.is_empty());
    }

    // rtmx:req REQ-TUI-040
    #[test]
    fn next_wraps_around() {
        let mut p = CommandPalette::new();
        p.filter("/");
        let len = p.filtered.len();
        for _ in 0..len {
            p.next();
        }
        assert_eq!(p.selected, 0);
    }

    // rtmx:req REQ-TUI-040
    #[test]
    fn prev_wraps_around() {
        let mut p = CommandPalette::new();
        p.filter("/");
        p.prev();
        assert_eq!(p.selected, p.filtered.len() - 1);
    }

    // rtmx:req REQ-TUI-040
    #[test]
    fn selected_command_returns_name() {
        let p = CommandPalette::new();
        assert_eq!(p.selected_command(), Some("/help"));
    }

    // rtmx:req REQ-TUI-040
    #[test]
    fn show_sets_visible() {
        let mut p = CommandPalette::new();
        p.show();
        assert!(p.is_visible);
    }

    // rtmx:req REQ-TUI-040
    #[test]
    fn hide_clears_visible() {
        let mut p = CommandPalette::new();
        p.show();
        p.hide();
        assert!(!p.is_visible);
    }

    // rtmx:req REQ-TUI-040
    #[test]
    fn view_returns_none_when_hidden() {
        let p = CommandPalette::new();
        assert!(p.view().is_none());
    }

    // rtmx:req REQ-TUI-063a
    #[test]
    fn test_connect_grammar_has_provider_then_model_then_region() {
        let g = connect_grammar();
        assert_eq!(g.name, "/connect");
        assert_eq!(g.slots.len(), 4);
        assert_eq!(g.slots[0].name, "provider");
        assert!(g.slots[0].required);
        assert_eq!(g.slots[1].name, "model");
        assert_eq!(g.slots[2].name, "region");
        assert_eq!(g.slots[3].name, "project");
    }

    // rtmx:req REQ-TUI-063a
    #[test]
    fn test_options_for_vertex_model_returns_gemini_variants() {
        let opts = options_for_provider("vertex", "model");
        let values: Vec<&str> = opts.iter().map(|o| o.value.as_str()).collect();
        assert!(values.contains(&"gemini-3.1-pro"));
        assert!(values.contains(&"gemini-3-flash"));
        assert!(values.contains(&"claude-opus-4.7"));
    }

    // rtmx:req REQ-TUI-063a
    #[test]
    fn test_options_for_bedrock_model_returns_claude() {
        let opts = options_for_provider("bedrock", "model");
        let values: Vec<&str> = opts.iter().map(|o| o.value.as_str()).collect();
        assert!(values.contains(&"claude-opus-sonnet-4.5"));
        assert!(values.contains(&"claude-haiku-3"));
    }

    // rtmx:req REQ-TUI-063a
    #[test]
    fn test_options_for_azure_model_returns_gpt() {
        let opts = options_for_provider("azure", "model");
        let values: Vec<&str> = opts.iter().map(|o| o.value.as_str()).collect();
        assert!(values.contains(&"gpt-5.1"));
        assert!(values.contains(&"gpt-4.1"));
        assert!(values.contains(&"o3-mini"));
    }

    // rtmx:req REQ-TUI-063a
    #[test]
    fn test_palette_stage_advances_on_selection() {
        let mut p = CommandPalette::new();
        p.show();
        let grammar = connect_grammar();
        p.enter_token_stage(grammar);
        assert!(p.in_token_stage());
        // First slot: provider options should be populated
        assert!(!p.filtered.is_empty());
        assert!(
            p.filtered.iter().any(|e| e.name == "vertex"),
            "should show vertex option"
        );
        // Advance past provider
        let has_more = p.advance_token("vertex".to_string());
        assert!(has_more, "should have model slot next");
        // Model options should now be vertex-specific
        assert!(
            p.filtered.iter().any(|e| e.name == "Gemini 3.1 Pro"),
            "should show vertex models"
        );
    }

    // rtmx:req REQ-TUI-063a
    #[test]
    fn test_palette_retreat_returns_to_previous_slot() {
        let mut p = CommandPalette::new();
        p.show();
        p.enter_token_stage(connect_grammar());
        p.advance_token("vertex".to_string());
        assert!(p.in_token_stage());
        // Retreat back to provider
        p.retreat_token();
        assert!(
            p.filtered.iter().any(|e| e.name == "vertex"),
            "should show provider options again"
        );
    }

    // rtmx:req REQ-TUI-063a
    #[test]
    fn test_palette_stage_hint_shows_slot_name() {
        let mut p = CommandPalette::new();
        p.show();
        p.enter_token_stage(connect_grammar());
        assert_eq!(p.stage_hint(), Some("Select provider:".to_string()));
        p.advance_token("vertex".to_string());
        assert_eq!(p.stage_hint(), Some("Select model:".to_string()));
    }

    // rtmx:req REQ-TUI-063a
    #[test]
    fn test_filter_token_narrows_options() {
        let mut p = CommandPalette::new();
        p.show();
        p.enter_token_stage(connect_grammar());
        p.filter_token("vert");
        assert_eq!(p.filtered.len(), 1);
        assert_eq!(p.filtered[0].name, "vertex");
    }

    // rtmx:req REQ-TUI-040
    #[test]
    fn view_returns_some_when_visible() {
        let mut p = CommandPalette::new();
        p.show();
        let v = p.view();
        assert!(v.is_some());
        assert_eq!(v.unwrap().entries.len(), p.all_commands.len());
    }

    // rtmx:req REQ-LLM-031
    #[test]
    fn test_inject_options_populates_slot() {
        let mut palette = CommandPalette::new();
        palette.show();
        let options = vec![
            TokenOption {
                value: "proj-1".into(),
                label: "Project One".into(),
                description: "proj-1".into(),
            },
            TokenOption {
                value: "proj-2".into(),
                label: "Project Two".into(),
                description: "proj-2".into(),
            },
        ];
        palette.inject_options("project", options);

        // Enter /connect grammar and advance to project slot
        let grammar = connect_grammar();
        palette.enter_token_stage(grammar);
        // Advance through provider, model, region to reach project
        palette.advance_token("vertex".into());
        palette.advance_token("gemini-3.1-pro".into());
        palette.advance_token("us-central1".into());
        // Now at project slot -- should show injected options
        let view = palette.view();
        assert!(view.is_some());
        let v = view.unwrap();
        assert!(v.entries.iter().any(|e| e.name == "Project One"));
        assert!(v.entries.iter().any(|e| e.name == "Project Two"));
    }

    // rtmx:req REQ-LLM-031
    #[test]
    fn test_clear_injected_removes_options() {
        let mut palette = CommandPalette::new();
        palette.show();
        palette.inject_options(
            "project",
            vec![TokenOption {
                value: "p".into(),
                label: "P".into(),
                description: "d".into(),
            }],
        );
        palette.clear_injected("project");
        // Verify the key is removed
        let grammar = connect_grammar();
        palette.enter_token_stage(grammar);
        palette.advance_token("vertex".into());
        palette.advance_token("gemini-3.1-pro".into());
        palette.advance_token("us-central1".into());
        // Should fall through to hardcoded (which is empty for project)
        // or show loading/empty state
        let view = palette.view();
        // Should not contain our previously-injected "P"
        if let Some(v) = view {
            assert!(!v.entries.iter().any(|e| e.name == "P"));
        }
    }

    // rtmx:req REQ-LLM-031
    #[test]
    fn test_connect_grammar_project_slot_is_enum() {
        let grammar = connect_grammar();
        let project_slot = &grammar.slots[3];
        assert_eq!(project_slot.name, "project");
        assert!(matches!(project_slot.kind, TokenKind::Enum(_)));
    }

    // rtmx:req REQ-LLM-031
    #[test]
    fn test_refresh_current_slot_updates_filtered() {
        let mut palette = CommandPalette::new();
        palette.show();
        let grammar = connect_grammar();
        palette.enter_token_stage(grammar);
        palette.advance_token("vertex".into());
        palette.advance_token("gemini-3.1-pro".into());
        palette.advance_token("us-central1".into());
        // Now at project slot with no injected options
        // Inject some options and refresh
        palette.inject_options(
            "project",
            vec![TokenOption {
                value: "new-proj".into(),
                label: "New Project".into(),
                description: "new-proj".into(),
            }],
        );
        palette.refresh_current_slot();
        let view = palette.view();
        assert!(view.is_some());
        assert!(
            view.unwrap()
                .entries
                .iter()
                .any(|e| e.name == "New Project")
        );
    }

    // rtmx:req REQ-LLM-031
    #[test]
    fn test_non_project_slots_unaffected_by_injection() {
        // Injecting options for "project" should not affect model/region slots
        let mut palette = CommandPalette::new();
        palette.show();
        palette.inject_options(
            "project",
            vec![TokenOption {
                value: "p".into(),
                label: "Injected".into(),
                description: "d".into(),
            }],
        );
        let grammar = connect_grammar();
        palette.enter_token_stage(grammar);
        // First slot is provider -- should show vertex/bedrock/azure/local, not injected
        let view = palette.view().unwrap();
        assert!(view.entries.iter().any(|e| e.name == "vertex"));
        assert!(!view.entries.iter().any(|e| e.name == "Injected"));
    }
}
