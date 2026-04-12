//! Simple markdown rendering for chat messages.
//!
//! Converts a subset of Markdown into styled ratatui `Line` objects:
//! - `**bold**` -> bold style
//! - `` `code` `` -> gray background style
//! - `# heading` -> bold + underline style
//! - Fenced code blocks (``` ```lang ... ``` ```) -> syntax-highlighted with dark background
//! - Plain text passes through unstyled.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style as SyntectStyle, ThemeSet};
use syntect::parsing::SyntaxSet;

/// Background color for fenced code blocks.
const CODE_BLOCK_BG: Color = Color::Rgb(30, 30, 30);

/// Style applied to language labels (REQ-TUI-043).
const LANG_LABEL_STYLE: Style = Style::new().fg(Color::DarkGray).bg(CODE_BLOCK_BG);

/// Render a markdown string into styled ratatui Lines.
pub fn render_markdown(text: &str) -> Vec<Line<'static>> {
    if text.is_empty() {
        return vec![Line::from(Span::raw(String::new()))];
    }

    let ss = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let theme = &ts.themes["base16-ocean.dark"];

    let mut lines_out: Vec<Line<'static>> = Vec::new();
    let mut in_code_block = false;
    let mut code_lang: Option<String> = None;
    let mut code_lines: Vec<String> = Vec::new();

    for line in text.lines() {
        if !in_code_block {
            if let Some(lang) = parse_fence_open(line) {
                in_code_block = true;
                code_lang = lang;
                code_lines.clear();
                continue;
            }
            lines_out.push(render_line(line));
        } else if line.trim_end() == "```" {
            // Close the fenced block -- emit highlighted lines
            let lang_tag = code_lang.take();
            let syntax = lang_tag
                .as_deref()
                .and_then(|l| resolve_syntax(&ss, l))
                .map(|idx| &ss.syntaxes()[idx]);

            // REQ-TUI-043: language label line
            if let Some(ref tag) = lang_tag {
                let label_span = Span::styled(tag.clone(), LANG_LABEL_STYLE);
                lines_out.push(Line::from(label_span));
            }

            if let Some(syn) = syntax {
                let mut h = HighlightLines::new(syn, theme);
                for code_line in &code_lines {
                    let regions = h.highlight_line(code_line, &ss).unwrap_or_default();
                    let spans = syntect_regions_to_spans(&regions, code_line);
                    lines_out.push(Line::from(spans));
                }
            } else {
                // Unknown language -- plain text with dark bg
                for code_line in &code_lines {
                    lines_out.push(Line::from(Span::styled(
                        code_line.clone(),
                        Style::default().bg(CODE_BLOCK_BG),
                    )));
                }
            }

            in_code_block = false;
            code_lines.clear();
        } else {
            code_lines.push(line.to_string());
        }
    }

    // Unclosed fence -- render accumulated lines as plain text
    if in_code_block {
        let fence_line = match &code_lang {
            Some(l) => format!("```{l}"),
            None => "```".to_string(),
        };
        lines_out.push(render_line(&fence_line));
        for code_line in &code_lines {
            lines_out.push(render_line(code_line));
        }
    }

    lines_out
}

/// Parse a fenced code block opening line. Returns `Some(Some(lang))` for
/// `` ```rust ``, `Some(None)` for bare `` ``` ``, or `None` if not a fence.
fn parse_fence_open(line: &str) -> Option<Option<String>> {
    let trimmed = line.trim_end();
    if !trimmed.starts_with("```") {
        return None;
    }
    let after = &trimmed[3..];
    // A closing fence on a line by itself is NOT an opening fence.
    if after.is_empty() && !trimmed.contains(char::is_alphanumeric) {
        // Bare ``` could be an open fence when we are NOT inside a block.
        // We treat bare ``` as an opening fence (no lang).
        return Some(None);
    }
    // Language tag: only alphanumeric, hyphens, plus (e.g. "c++")
    let lang = after.trim();
    if lang.is_empty() {
        return Some(None);
    }
    if lang
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '+' || c == '#')
    {
        Some(Some(lang.to_lowercase()))
    } else {
        // Not a valid fence (e.g. inline triple backticks with text)
        None
    }
}

/// Resolve a language tag to a syntect syntax index.
fn resolve_syntax(ss: &SyntaxSet, lang: &str) -> Option<usize> {
    // Try direct name match first, then extension-based
    let lower = lang.to_lowercase();
    let aliases: &[&str] = match lower.as_str() {
        "rust" => &["rs"],
        "python" | "py" => &["py"],
        "typescript" | "ts" => &["ts"],
        "javascript" | "js" => &["js"],
        "bash" | "sh" | "shell" | "zsh" => &["sh"],
        "yaml" | "yml" => &["yaml"],
        "toml" => &["toml"],
        "json" => &["json"],
        "sql" => &["sql"],
        "c" => &["c"],
        "cpp" | "c++" | "cxx" => &["cpp"],
        "java" => &["java"],
        "go" | "golang" => &["go"],
        "ruby" | "rb" => &["rb"],
        "html" => &["html"],
        "css" => &["css"],
        "xml" => &["xml"],
        "markdown" | "md" => &["md"],
        _ => &[],
    };

    // Try the language name directly
    if let Some(idx) = ss
        .syntaxes()
        .iter()
        .position(|s| s.name.eq_ignore_ascii_case(&lower))
    {
        return Some(idx);
    }

    // Try aliases as file extensions
    for ext in aliases {
        if let Some(syn) = ss.find_syntax_by_extension(ext)
            && let Some(idx) = ss.syntaxes().iter().position(|s| s.name == syn.name)
        {
            return Some(idx);
        }
    }

    // Last resort: try as extension directly
    if let Some(syn) = ss.find_syntax_by_extension(&lower) {
        return ss.syntaxes().iter().position(|s| s.name == syn.name);
    }

    None
}

/// Convert syntect highlighted regions into ratatui Spans with code block bg.
fn syntect_regions_to_spans(
    regions: &[(SyntectStyle, &str)],
    _original: &str,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for (style, text) in regions {
        let fg = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
        spans.push(Span::styled(
            text.to_string(),
            Style::default().fg(fg).bg(CODE_BLOCK_BG),
        ));
    }
    if spans.is_empty() {
        spans.push(Span::styled(
            String::new(),
            Style::default().bg(CODE_BLOCK_BG),
        ));
    }
    spans
}

fn render_line(line: &str) -> Line<'static> {
    // Check for heading
    if let Some(heading_text) = line.strip_prefix("# ") {
        return Line::from(Span::styled(
            heading_text.to_string(),
            Style::default()
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::UNDERLINED),
        ));
    }

    // Parse inline bold and code spans
    let spans = parse_inline(line);
    Line::from(spans)
}

enum Marker {
    Bold(usize),
    Code(usize),
}

fn find_next_marker(text: &str) -> Option<Marker> {
    let bold_pos = text.find("**");
    let code_pos = text.find('`');
    match (bold_pos, code_pos) {
        (Some(b), Some(c)) if b <= c => Some(Marker::Bold(b)),
        (Some(_), Some(c)) => Some(Marker::Code(c)),
        (Some(b), None) => Some(Marker::Bold(b)),
        (None, Some(c)) => Some(Marker::Code(c)),
        (None, None) => None,
    }
}

fn parse_inline(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        match find_next_marker(remaining) {
            None => {
                spans.push(Span::raw(remaining.to_string()));
                return spans;
            }
            Some(Marker::Bold(pos)) => {
                if pos > 0 {
                    spans.push(Span::raw(remaining[..pos].to_string()));
                }
                let after_open = &remaining[pos + 2..];
                if let Some(close) = after_open.find("**") {
                    spans.push(Span::styled(
                        after_open[..close].to_string(),
                        Style::default().add_modifier(Modifier::BOLD),
                    ));
                    remaining = &after_open[close + 2..];
                } else {
                    spans.push(Span::raw(remaining[pos..].to_string()));
                    return spans;
                }
            }
            Some(Marker::Code(pos)) => {
                if pos > 0 {
                    spans.push(Span::raw(remaining[..pos].to_string()));
                }
                let after_open = &remaining[pos + 1..];
                if let Some(close) = after_open.find('`') {
                    spans.push(Span::styled(
                        after_open[..close].to_string(),
                        Style::default().bg(Color::DarkGray),
                    ));
                    remaining = &after_open[close + 1..];
                } else {
                    spans.push(Span::raw(remaining[pos..].to_string()));
                    return spans;
                }
            }
        }
    }

    if spans.is_empty() {
        spans.push(Span::raw(String::new()));
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-TUI-002
    #[test]
    fn plain_text_passes_through() {
        let lines = render_markdown("Hello world");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans.len(), 1);
        assert_eq!(lines[0].spans[0].content, "Hello world");
        assert_eq!(lines[0].spans[0].style, Style::default());
    }

    // rtmx:req REQ-TUI-002
    #[test]
    fn bold_text_rendered_bold() {
        let lines = render_markdown("This is **bold** text");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans.len(), 3);
        assert_eq!(lines[0].spans[0].content, "This is ");
        assert_eq!(lines[0].spans[1].content, "bold");
        assert!(
            lines[0].spans[1]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
        assert_eq!(lines[0].spans[2].content, " text");
    }

    // rtmx:req REQ-TUI-002
    #[test]
    fn code_span_rendered_with_gray_background() {
        let lines = render_markdown("Run `cargo test` now");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans.len(), 3);
        assert_eq!(lines[0].spans[0].content, "Run ");
        assert_eq!(lines[0].spans[1].content, "cargo test");
        assert_eq!(
            lines[0].spans[1].style,
            Style::default().bg(Color::DarkGray)
        );
        assert_eq!(lines[0].spans[2].content, " now");
    }

    // rtmx:req REQ-TUI-002
    #[test]
    fn heading_rendered_bold_underline() {
        let lines = render_markdown("# My Heading");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans.len(), 1);
        assert_eq!(lines[0].spans[0].content, "My Heading");
        let style = lines[0].spans[0].style;
        assert!(style.add_modifier.contains(Modifier::BOLD));
        assert!(style.add_modifier.contains(Modifier::UNDERLINED));
    }

    // rtmx:req REQ-TUI-002
    #[test]
    fn multiline_text_produces_multiple_lines() {
        let lines = render_markdown("Line one\nLine two\nLine three");
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].spans[0].content, "Line one");
        assert_eq!(lines[1].spans[0].content, "Line two");
        assert_eq!(lines[2].spans[0].content, "Line three");
    }

    // rtmx:req REQ-TUI-002
    #[test]
    fn mixed_bold_and_code() {
        let lines = render_markdown("Use **bold** and `code` together");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans.len(), 5);
        assert_eq!(lines[0].spans[0].content, "Use ");
        assert_eq!(lines[0].spans[1].content, "bold");
        assert!(
            lines[0].spans[1]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
        assert_eq!(lines[0].spans[2].content, " and ");
        assert_eq!(lines[0].spans[3].content, "code");
        assert_eq!(
            lines[0].spans[3].style,
            Style::default().bg(Color::DarkGray)
        );
        assert_eq!(lines[0].spans[4].content, " together");
    }

    // rtmx:req REQ-TUI-002
    #[test]
    fn unclosed_bold_treated_as_plain() {
        let lines = render_markdown("This is **unclosed");
        assert_eq!(lines.len(), 1);
        // Should contain the raw text since ** is not closed
        let full_text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(full_text, "This is **unclosed");
    }

    // rtmx:req REQ-TUI-002
    #[test]
    fn unclosed_code_treated_as_plain() {
        let lines = render_markdown("Run `unclosed");
        assert_eq!(lines.len(), 1);
        let full_text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(full_text, "Run `unclosed");
    }

    // rtmx:req REQ-TUI-002
    #[test]
    fn empty_input_produces_single_empty_line() {
        let lines = render_markdown("");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans[0].content, "");
    }

    // rtmx:req REQ-TUI-002
    #[test]
    fn heading_among_other_lines() {
        let lines = render_markdown("# Title\nSome body text");
        assert_eq!(lines.len(), 2);
        assert!(
            lines[0].spans[0]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
        assert_eq!(lines[1].spans[0].style, Style::default());
    }

    // ---- REQ-TUI-041: Fenced code block detection ----

    // rtmx:req REQ-TUI-041
    #[test]
    fn fenced_code_block_no_lang_renders_with_dark_bg() {
        let input = "before\n```\nhello world\n```\nafter";
        let lines = render_markdown(input);
        // before, code line, after = 3 lines
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].spans[0].content, "before");
        // The code line should have dark background
        let code_span = &lines[1].spans[0];
        assert_eq!(code_span.content, "hello world");
        assert_eq!(code_span.style.bg, Some(CODE_BLOCK_BG));
        assert_eq!(lines[2].spans[0].content, "after");
    }

    // rtmx:req REQ-TUI-041
    #[test]
    fn fenced_code_block_preserves_whitespace() {
        let input = "```\n  indented\n    more indented\n```";
        let lines = render_markdown(input);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].spans[0].content, "  indented");
        assert_eq!(lines[1].spans[0].content, "    more indented");
    }

    // rtmx:req REQ-TUI-041
    #[test]
    fn fenced_code_block_with_lang_tag() {
        let input = "```python\nprint('hi')\n```";
        let lines = render_markdown(input);
        // lang label + 1 code line = 2 lines
        assert!(lines.len() >= 2);
        // First line should be language label
        assert_eq!(lines[0].spans[0].content, "python");
    }

    // rtmx:req REQ-TUI-041
    #[test]
    fn unclosed_fenced_block_rendered_as_plain_text() {
        let input = "before\n```rust\nfn main() {}\nno closing fence";
        let lines = render_markdown(input);
        // Should render as: before, ```rust, fn main() {}, no closing fence
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0].spans[0].content, "before");
        // The unclosed fence renders as plain text
        let fence_text: String = lines[1].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(fence_text.contains("```rust") || fence_text.contains("rust"));
    }

    // rtmx:req REQ-TUI-041
    #[test]
    fn multiple_fenced_blocks() {
        let input = "```\nblock one\n```\ntext between\n```\nblock two\n```";
        let lines = render_markdown(input);
        // block one line, text between, block two line = 3
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].spans[0].style.bg, Some(CODE_BLOCK_BG));
        assert_eq!(lines[1].spans[0].style, Style::default());
        assert_eq!(lines[2].spans[0].style.bg, Some(CODE_BLOCK_BG));
    }

    // rtmx:req REQ-TUI-041
    #[test]
    fn inline_backticks_inside_fenced_block_preserved() {
        let input = "```\nlet x = `backtick`;\n```";
        let lines = render_markdown(input);
        assert_eq!(lines.len(), 1);
        // Content should contain the backticks as-is (not parsed as inline code)
        let full_text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(full_text.contains("`backtick`"));
    }

    // rtmx:req REQ-TUI-041
    #[test]
    fn empty_fenced_block() {
        let input = "```\n```";
        let lines = render_markdown(input);
        // Empty block produces no code lines
        assert_eq!(lines.len(), 0);
    }

    // rtmx:req REQ-TUI-041
    #[test]
    fn fenced_block_all_lines_have_dark_bg() {
        let input = "```\nline1\nline2\nline3\n```";
        let lines = render_markdown(input);
        assert_eq!(lines.len(), 3);
        for line in &lines {
            assert_eq!(
                line.spans[0].style.bg,
                Some(CODE_BLOCK_BG),
                "All code block lines must have dark background"
            );
        }
    }

    // ---- REQ-TUI-042: Syntax highlighting ----

    // rtmx:req REQ-TUI-042
    #[test]
    fn rust_code_block_has_colored_spans() {
        let input = "```rust\nfn main() {\n    println!(\"hello\");\n}\n```";
        let lines = render_markdown(input);
        // Skip the language label line (index 0)
        assert!(lines.len() >= 2);
        // Code lines should have syntax-highlighted spans with non-default fg
        let code_line = &lines[1]; // "fn main() {"
        let has_colored_fg = code_line
            .spans
            .iter()
            .any(|s| matches!(s.style.fg, Some(Color::Rgb(_, _, _))));
        assert!(
            has_colored_fg,
            "Rust code should have colored foreground spans from syntax highlighting"
        );
    }

    // rtmx:req REQ-TUI-042
    #[test]
    fn python_code_block_has_colored_spans() {
        let input = "```python\ndef hello():\n    print('world')\n```";
        let lines = render_markdown(input);
        assert!(lines.len() >= 2);
        let code_line = &lines[1]; // "def hello():"
        let has_colored_fg = code_line
            .spans
            .iter()
            .any(|s| matches!(s.style.fg, Some(Color::Rgb(_, _, _))));
        assert!(
            has_colored_fg,
            "Python code should have colored foreground spans"
        );
    }

    // rtmx:req REQ-TUI-042
    #[test]
    fn unknown_language_falls_back_to_plain_with_bg() {
        let input = "```unknownlang\nsome code here\n```";
        let lines = render_markdown(input);
        // lang label + 1 code line
        assert!(lines.len() >= 2);
        // The code line should have dark bg but no syntax coloring
        let code_line = &lines[1];
        assert_eq!(code_line.spans.len(), 1);
        assert_eq!(code_line.spans[0].style.bg, Some(CODE_BLOCK_BG));
    }

    // rtmx:req REQ-TUI-042
    #[test]
    fn all_highlighted_spans_have_code_block_bg() {
        let input = "```rust\nlet x = 42;\n```";
        let lines = render_markdown(input);
        // Skip lang label (index 0), check code line (index 1)
        let code_line = &lines[1];
        for span in &code_line.spans {
            assert_eq!(
                span.style.bg,
                Some(CODE_BLOCK_BG),
                "Every span in a highlighted block must have the code block background"
            );
        }
    }

    // rtmx:req REQ-TUI-042
    #[test]
    fn javascript_syntax_highlighting() {
        let input = "```javascript\nconst x = 'hello';\n```";
        let lines = render_markdown(input);
        assert!(lines.len() >= 2);
        let code_line = &lines[1];
        let has_colored = code_line
            .spans
            .iter()
            .any(|s| matches!(s.style.fg, Some(Color::Rgb(_, _, _))));
        assert!(has_colored, "JavaScript code should be syntax highlighted");
    }

    // rtmx:req REQ-TUI-042
    #[test]
    fn bash_syntax_highlighting() {
        let input = "```bash\necho \"hello world\"\n```";
        let lines = render_markdown(input);
        assert!(lines.len() >= 2);
        let code_line = &lines[1];
        let has_colored = code_line
            .spans
            .iter()
            .any(|s| matches!(s.style.fg, Some(Color::Rgb(_, _, _))));
        assert!(has_colored, "Bash code should be syntax highlighted");
    }

    // rtmx:req REQ-TUI-042
    #[test]
    fn go_syntax_highlighting() {
        let input = "```go\nfunc main() {\n    fmt.Println(\"hello\")\n}\n```";
        let lines = render_markdown(input);
        assert!(lines.len() >= 2);
        let code_line = &lines[1];
        let has_colored = code_line
            .spans
            .iter()
            .any(|s| matches!(s.style.fg, Some(Color::Rgb(_, _, _))));
        assert!(has_colored, "Go code should be syntax highlighted");
    }

    // ---- REQ-TUI-043: Language label ----

    // rtmx:req REQ-TUI-043
    #[test]
    fn language_label_displayed_for_tagged_blocks() {
        let input = "```rust\nlet x = 1;\n```";
        let lines = render_markdown(input);
        // First line should be the language label
        assert!(lines.len() >= 2);
        assert_eq!(lines[0].spans[0].content, "rust");
        assert_eq!(lines[0].spans[0].style, LANG_LABEL_STYLE);
    }

    // rtmx:req REQ-TUI-043
    #[test]
    fn no_language_label_for_untagged_blocks() {
        let input = "```\nsome code\n```";
        let lines = render_markdown(input);
        // No language label -- just the code line
        assert_eq!(lines.len(), 1);
        // The first (and only) line should be the code, not a label
        let full: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(full, "some code");
    }

    // rtmx:req REQ-TUI-043
    #[test]
    fn language_label_is_lowercased() {
        let input = "```Python\npass\n```";
        let lines = render_markdown(input);
        assert!(lines.len() >= 2);
        assert_eq!(lines[0].spans[0].content, "python");
    }

    // ---- parse_fence_open unit tests ----

    // rtmx:req REQ-TUI-041
    #[test]
    fn parse_fence_open_bare() {
        assert_eq!(parse_fence_open("```"), Some(None));
    }

    // rtmx:req REQ-TUI-041
    #[test]
    fn parse_fence_open_with_lang() {
        assert_eq!(parse_fence_open("```rust"), Some(Some("rust".to_string())));
    }

    // rtmx:req REQ-TUI-041
    #[test]
    fn parse_fence_open_with_lang_trailing_space() {
        assert_eq!(
            parse_fence_open("```rust  "),
            Some(Some("rust".to_string()))
        );
    }

    // rtmx:req REQ-TUI-041
    #[test]
    fn parse_fence_open_not_a_fence() {
        assert_eq!(parse_fence_open("hello"), None);
    }

    // rtmx:req REQ-TUI-041
    #[test]
    fn parse_fence_open_cpp_language() {
        assert_eq!(parse_fence_open("```c++"), Some(Some("c++".to_string())));
    }

    // rtmx:req REQ-TUI-041
    #[test]
    fn parse_fence_open_csharp_language() {
        assert_eq!(parse_fence_open("```c#"), Some(Some("c#".to_string())));
    }

    // rtmx:req REQ-TUI-042
    #[test]
    fn resolve_syntax_returns_some_for_rust() {
        let ss = SyntaxSet::load_defaults_newlines();
        assert!(
            resolve_syntax(&ss, "rust").is_some(),
            "Rust syntax should be found"
        );
    }

    // rtmx:req REQ-TUI-042
    #[test]
    fn resolve_syntax_returns_some_for_python() {
        let ss = SyntaxSet::load_defaults_newlines();
        assert!(
            resolve_syntax(&ss, "python").is_some(),
            "Python syntax should be found"
        );
    }

    // rtmx:req REQ-TUI-042
    #[test]
    fn resolve_syntax_returns_none_for_unknown() {
        let ss = SyntaxSet::load_defaults_newlines();
        assert!(
            resolve_syntax(&ss, "unknownlang123").is_none(),
            "Unknown language should return None"
        );
    }
}
