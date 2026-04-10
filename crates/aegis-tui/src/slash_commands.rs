//! Slash command parsing for TUI input.
//!
//! Parses input strings starting with `/` into known commands.
//! Non-slash input returns `None`. Unknown slash commands return an error.

/// Recognized slash commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashCommand {
    /// Clear the chat log.
    Clear,
    /// Show help text.
    Help,
    /// Show current context summary.
    Context,
    /// Quit the application.
    Quit,
    /// Add a file to the agent's context window.
    Add(String),
    /// Remove a file from the agent's context window.
    Drop(String),
}

/// Result of attempting to parse a slash command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseResult {
    /// A recognized slash command.
    Command(SlashCommand),
    /// Input did not start with `/` -- not a slash command.
    NotACommand,
    /// Input started with `/` but the command is unknown.
    Unknown(String),
}

/// Parse an input string as a potential slash command.
///
/// Returns `ParseResult::Command` for known commands, `ParseResult::NotACommand`
/// for input that does not start with `/`, and `ParseResult::Unknown` for
/// unrecognized slash commands.
pub fn parse_slash_command(input: &str) -> ParseResult {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return ParseResult::NotACommand;
    }

    let cmd = trimmed
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_lowercase();

    // Extract the argument (everything after the first word).
    let arg = trimmed
        .split_whitespace()
        .skip(1)
        .collect::<Vec<_>>()
        .join(" ");

    match cmd.as_str() {
        "/clear" => ParseResult::Command(SlashCommand::Clear),
        "/help" => ParseResult::Command(SlashCommand::Help),
        "/context" => ParseResult::Command(SlashCommand::Context),
        "/quit" => ParseResult::Command(SlashCommand::Quit),
        "/add" => {
            if arg.is_empty() {
                ParseResult::Command(SlashCommand::Add(String::new()))
            } else {
                ParseResult::Command(SlashCommand::Add(arg))
            }
        }
        "/drop" => {
            if arg.is_empty() {
                ParseResult::Command(SlashCommand::Drop(String::new()))
            } else {
                ParseResult::Command(SlashCommand::Drop(arg))
            }
        }
        _ => ParseResult::Unknown(cmd),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // @req REQ-TUI-010
    #[test]
    fn parse_clear_command() {
        assert_eq!(
            parse_slash_command("/clear"),
            ParseResult::Command(SlashCommand::Clear)
        );
    }

    // @req REQ-TUI-010
    #[test]
    fn parse_help_command() {
        assert_eq!(
            parse_slash_command("/help"),
            ParseResult::Command(SlashCommand::Help)
        );
    }

    // @req REQ-TUI-010
    #[test]
    fn parse_context_command() {
        assert_eq!(
            parse_slash_command("/context"),
            ParseResult::Command(SlashCommand::Context)
        );
    }

    // @req REQ-TUI-010
    #[test]
    fn parse_quit_command() {
        assert_eq!(
            parse_slash_command("/quit"),
            ParseResult::Command(SlashCommand::Quit)
        );
    }

    // @req REQ-TUI-010
    #[test]
    fn unknown_slash_command_returns_error() {
        assert_eq!(
            parse_slash_command("/foobar"),
            ParseResult::Unknown("/foobar".to_string())
        );
    }

    // @req REQ-TUI-010
    #[test]
    fn non_slash_input_returns_none() {
        assert_eq!(parse_slash_command("hello world"), ParseResult::NotACommand);
    }

    // @req REQ-TUI-010
    #[test]
    fn empty_input_returns_not_a_command() {
        assert_eq!(parse_slash_command(""), ParseResult::NotACommand);
    }

    // @req REQ-TUI-010
    #[test]
    fn whitespace_only_returns_not_a_command() {
        assert_eq!(parse_slash_command("   "), ParseResult::NotACommand);
    }

    // @req REQ-TUI-010
    #[test]
    fn slash_command_with_leading_whitespace() {
        assert_eq!(
            parse_slash_command("  /clear"),
            ParseResult::Command(SlashCommand::Clear)
        );
    }

    // @req REQ-TUI-010
    #[test]
    fn slash_command_case_insensitive() {
        assert_eq!(
            parse_slash_command("/CLEAR"),
            ParseResult::Command(SlashCommand::Clear)
        );
        assert_eq!(
            parse_slash_command("/Help"),
            ParseResult::Command(SlashCommand::Help)
        );
    }

    // @req REQ-TUI-010
    #[test]
    fn slash_command_with_trailing_args_still_parses() {
        assert_eq!(
            parse_slash_command("/help something"),
            ParseResult::Command(SlashCommand::Help)
        );
    }

    // @req REQ-TUI-010
    #[test]
    fn bare_slash_is_unknown() {
        assert_eq!(
            parse_slash_command("/"),
            ParseResult::Unknown("/".to_string())
        );
    }

    // @req REQ-TUI-024
    #[test]
    fn parse_add_command_with_path() {
        assert_eq!(
            parse_slash_command("/add src/main.rs"),
            ParseResult::Command(SlashCommand::Add("src/main.rs".to_string()))
        );
    }

    // @req REQ-TUI-024
    #[test]
    fn parse_add_command_without_path() {
        assert_eq!(
            parse_slash_command("/add"),
            ParseResult::Command(SlashCommand::Add(String::new()))
        );
    }

    // @req REQ-TUI-024
    #[test]
    fn parse_drop_command_with_path() {
        assert_eq!(
            parse_slash_command("/drop src/main.rs"),
            ParseResult::Command(SlashCommand::Drop("src/main.rs".to_string()))
        );
    }

    // @req REQ-TUI-024
    #[test]
    fn parse_drop_command_without_path() {
        assert_eq!(
            parse_slash_command("/drop"),
            ParseResult::Command(SlashCommand::Drop(String::new()))
        );
    }

    // @req REQ-TUI-024
    #[test]
    fn parse_add_case_insensitive() {
        assert_eq!(
            parse_slash_command("/ADD readme.md"),
            ParseResult::Command(SlashCommand::Add("readme.md".to_string()))
        );
    }
}
