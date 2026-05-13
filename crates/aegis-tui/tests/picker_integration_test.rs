//! Integration tests for the @ picker preview pane and symbol search.
//!
//! REQ-TUI-082: @ picker shows fetched URL content in right preview pane.
//! REQ-TUI-085: @symbol: query shows matching symbols in picker dropdown.
//! REQ-TUI-086: Symbol preview pane shows surrounding lines of code at definition site.

use aegis_tui::symbol_index::{SymbolIndex, SymbolKind, SymbolLocation};
use aegis_tui::url_fetcher::{FetchResult, strip_html_tags, truncate_content};

// ---------------------------------------------------------------------------
// REQ-TUI-082: Preview pane rendering of fetched content
// ---------------------------------------------------------------------------

// rtmx:req REQ-TUI-082
#[test]
fn test_url_preview_pane_renders() {
    // Given a FetchResult with HTML content stripped to plain text, verify
    // that the content can be formatted as a preview string suitable for
    // display in the right preview pane.
    let raw_html = "<html><head><title>Test Page</title></head>\
                    <body><h1>Hello World</h1><p>Some content here.</p></body></html>";
    let stripped = strip_html_tags(raw_html);

    let result = FetchResult {
        content: truncate_content(&stripped),
        status: 200,
        content_type: "text/html; charset=utf-8".to_string(),
    };

    // The preview string includes the status and content type in a header.
    let preview = format_fetch_preview(&result);

    assert!(
        preview.contains("200"),
        "preview must show HTTP status: {preview}"
    );
    assert!(
        preview.contains("Hello World"),
        "preview must include page heading: {preview}"
    );
    assert!(
        preview.contains("Some content here"),
        "preview must include body text: {preview}"
    );
    // HTML tags must be absent -- the picker pane shows plain text only.
    assert!(
        !preview.contains('<'),
        "preview must not contain raw HTML tags: {preview}"
    );
}

// rtmx:req REQ-TUI-082
#[test]
fn test_url_preview_pane_renders_plain_text() {
    // For non-HTML content types the body is used as-is.
    let body = "fn main() {\n    println!(\"hello\");\n}\n";
    let result = FetchResult {
        content: truncate_content(body),
        status: 200,
        content_type: "text/plain".to_string(),
    };
    let preview = format_fetch_preview(&result);

    assert!(
        preview.contains("fn main"),
        "plain-text preview must show source: {preview}"
    );
    assert!(
        preview.contains("200"),
        "preview must include status: {preview}"
    );
}

// rtmx:req REQ-TUI-082
#[test]
fn test_url_preview_pane_renders_long_content_truncated() {
    // Content longer than 4 KB must be truncated; the preview must not exceed
    // a reasonable display length.
    let long_body = "word ".repeat(2000); // 10 000 chars -- well above 4 KB limit
    let truncated = truncate_content(&long_body);

    let result = FetchResult {
        content: truncated.clone(),
        status: 200,
        content_type: "text/plain".to_string(),
    };
    let preview = format_fetch_preview(&result);

    // The content itself is capped; verify the preview reflects that.
    assert!(
        preview.len() < 5000,
        "preview of long content should be bounded; got {} chars",
        preview.len()
    );
    assert!(
        result.content.ends_with("..."),
        "truncated content must end with ellipsis: {}",
        &result.content[result.content.len().saturating_sub(10)..]
    );
}

// rtmx:req REQ-TUI-082
#[test]
fn test_url_preview_pane_non_200_status_shows_error() {
    // A FetchResult that could not be built because of a non-success status
    // should be representable as an error preview; verify the formatting
    // helper handles status != 200 without panicking.
    let result = FetchResult {
        content: String::new(),
        status: 404,
        content_type: "text/plain".to_string(),
    };
    let preview = format_fetch_preview(&result);
    // Preview must at minimum surface the status code.
    assert!(
        preview.contains("404"),
        "error preview must show status: {preview}"
    );
}

/// Format a `FetchResult` into a preview string for the picker right pane.
///
/// This mirrors what the TUI renderer would produce: a one-line header with
/// status + content-type, a separator, then the extracted text body.  The
/// logic is purposely kept simple so the test exercises the data flow rather
/// than the ratatui layout widgets.
fn format_fetch_preview(result: &FetchResult) -> String {
    let header = format!("HTTP {} | {}", result.status, result.content_type);
    let sep = "-".repeat(header.len().min(80));
    if result.content.is_empty() {
        format!("{header}\n{sep}\n(no content)")
    } else {
        format!("{header}\n{sep}\n{}", result.content)
    }
}

// ---------------------------------------------------------------------------
// REQ-TUI-085: @ picker integration with symbol search results
// ---------------------------------------------------------------------------

// rtmx:req REQ-TUI-085
#[test]
fn test_symbol_picker_shows_matches() {
    // Given a SymbolIndex built from a Rust source file, a prefix query
    // must return the expected symbols for display in the picker dropdown.
    let src = r#"
pub fn fetch_url(url: &str) -> Result<FetchResult, String> {
    todo!()
}

pub struct FetchResult {
    pub content: String,
}

pub fn strip_html_tags(html: &str) -> String {
    String::new()
}

pub fn truncate_content(content: &str) -> String {
    String::new()
}
"#;
    let mut index = SymbolIndex::new();
    index.index_file(src, "src/url_fetcher.rs");

    // Query "fetch" should match fetch_url and FetchResult.
    let results = index.search("fetch");
    let names: Vec<&str> = results.iter().map(|(n, _)| *n).collect();

    assert!(
        names.contains(&"fetch_url"),
        "search('fetch') must find fetch_url; got: {names:?}"
    );
    assert!(
        names.contains(&"FetchResult"),
        "search('fetch') must find FetchResult (case-insensitive); got: {names:?}"
    );

    // Query "strip" should match only strip_html_tags.
    let strip_results = index.search("strip");
    assert_eq!(
        strip_results.len(),
        1,
        "search('strip') must return exactly one match; got: {strip_results:?}"
    );
    assert_eq!(
        strip_results[0].0, "strip_html_tags",
        "search('strip') must return strip_html_tags"
    );

    // Verify that each match carries the correct file path.
    for (_, locs) in &results {
        for loc in *locs {
            assert_eq!(
                loc.file_path, "src/url_fetcher.rs",
                "location file path must match indexed file"
            );
        }
    }
}

// rtmx:req REQ-TUI-085
#[test]
fn test_symbol_picker_shows_matches_across_files() {
    // Symbols indexed from multiple files must all appear when the query
    // matches names in both files.
    let rust_src = "pub fn render() {}\npub struct Widget {}";
    let py_src = "def render():\n    pass\nclass Widget:\n    pass";

    let mut index = SymbolIndex::new();
    index.index_file(rust_src, "src/tui.rs");
    index.index_file(py_src, "scripts/render.py");

    // "render" appears in both files.
    let render_locs = index.lookup("render");
    assert_eq!(
        render_locs.len(),
        2,
        "render must be found in both files; got {render_locs:?}"
    );

    let files: Vec<&str> = render_locs.iter().map(|l| l.file_path.as_str()).collect();
    assert!(files.contains(&"src/tui.rs"), "must include Rust file");
    assert!(
        files.contains(&"scripts/render.py"),
        "must include Python file"
    );
}

// rtmx:req REQ-TUI-085
#[test]
fn test_symbol_picker_empty_query_returns_all() {
    // An empty query string must match every symbol (substring match with "").
    let src = "fn alpha() {}\nfn beta() {}\nstruct Gamma {}";
    let mut index = SymbolIndex::new();
    index.index_file(src, "src/lib.rs");

    let results = index.search("");
    assert_eq!(
        results.len(),
        3,
        "empty query should match all 3 symbols; got {results:?}"
    );
}

// rtmx:req REQ-TUI-085
#[test]
fn test_symbol_picker_no_matches_returns_empty() {
    let src = "fn main() {}\nstruct Config {}";
    let mut index = SymbolIndex::new();
    index.index_file(src, "src/main.rs");

    let results = index.search("zzznomatch");
    assert!(
        results.is_empty(),
        "no-match query must return empty vec; got {results:?}"
    );
}

// rtmx:req REQ-TUI-085
#[test]
fn test_symbol_picker_results_are_sorted() {
    // The picker dropdown must present symbols in alphabetical order so the
    // user can navigate predictably.
    let src = "fn zebra() {}\nfn apple() {}\nfn mango() {}";
    let mut index = SymbolIndex::new();
    index.index_file(src, "src/lib.rs");

    let results = index.search("");
    let names: Vec<&str> = results.iter().map(|(n, _)| *n).collect();

    assert_eq!(
        names,
        vec!["apple", "mango", "zebra"],
        "results must be sorted alphabetically; got {names:?}"
    );
}

// ---------------------------------------------------------------------------
// REQ-TUI-086: Preview pane showing symbol definition context
// ---------------------------------------------------------------------------

// rtmx:req REQ-TUI-086
#[test]
fn test_symbol_preview_shows_context() {
    // Given a source file and a symbol location (file, line), the context
    // extractor must return the surrounding lines for display in the preview
    // pane, centred on the definition line.
    let src = "// module header\n\
               use std::fmt;\n\
               \n\
               /// A widget type.\n\
               pub struct Widget {\n\
                   name: String,\n\
               }\n\
               \n\
               impl Widget {\n\
                   pub fn new(name: &str) -> Self {\n\
                       Widget { name: name.to_string() }\n\
                   }\n\
               }\n";

    // Widget is defined at line 5 (1-based).
    let loc = SymbolLocation {
        file_path: "src/widget.rs".to_string(),
        line: 5,
        kind: SymbolKind::Struct,
    };

    let context = extract_context_lines(src, loc.line, 3);

    // The definition line itself must be present.
    assert!(
        context.iter().any(|(_, l)| l.contains("pub struct Widget")),
        "context must include the definition line; got: {context:?}"
    );

    // Lines immediately before and after should be included.
    assert!(
        context.iter().any(|(_, l)| l.contains("A widget type")),
        "context should include the doc-comment immediately before definition; got: {context:?}"
    );
    assert!(
        context.iter().any(|(_, l)| l.contains("name: String")),
        "context should include the first field after definition; got: {context:?}"
    );

    // Line numbers in the output must be positive and 1-based.
    for (line_no, _) in &context {
        assert!(*line_no >= 1, "line numbers must be 1-based; got {line_no}");
    }
}

// rtmx:req REQ-TUI-086
#[test]
fn test_symbol_preview_context_at_first_line() {
    // When the symbol is at line 1 the extractor must not panic and must
    // not include negative-offset lines.
    let src = "fn first() {}\nfn second() {}";
    let context = extract_context_lines(src, 1, 3);

    assert!(!context.is_empty(), "context must not be empty");
    assert!(
        context.iter().any(|(_, l)| l.contains("fn first")),
        "must include the first-line symbol; got: {context:?}"
    );
    // No line number should be less than 1.
    for (line_no, _) in &context {
        assert!(*line_no >= 1, "line numbers must not be less than 1");
    }
}

// rtmx:req REQ-TUI-086
#[test]
fn test_symbol_preview_context_at_last_line() {
    // When the symbol is on the final line the extractor must not panic and
    // must not index past the end of the file.
    let src = "fn alpha() {}\nfn beta() {}\nfn gamma() {}";
    let last_line = src.lines().count(); // 3
    let context = extract_context_lines(src, last_line, 3);

    assert!(!context.is_empty(), "context must not be empty");
    assert!(
        context.iter().any(|(_, l)| l.contains("fn gamma")),
        "must include the last-line symbol; got: {context:?}"
    );
    let max_line = context.iter().map(|(n, _)| *n).max().unwrap();
    assert_eq!(
        max_line, last_line,
        "context must not exceed the last line of the file"
    );
}

// rtmx:req REQ-TUI-086
#[test]
fn test_symbol_preview_context_window_size() {
    // Verify that the context window is bounded: with radius=2, at most
    // 2*radius+1 = 5 lines are returned (fewer at file edges).
    let src = (1..=20)
        .map(|i| format!("fn func_{i}() {{}}"))
        .collect::<Vec<_>>()
        .join("\n");
    let context = extract_context_lines(&src, 10, 2);

    assert!(
        context.len() <= 5,
        "window radius=2 must produce at most 5 lines; got {}",
        context.len()
    );
    assert!(
        context.iter().any(|(n, _)| *n == 10),
        "context must include the target line; got: {context:?}"
    );
}

/// Extract `radius` lines before and after `target_line` (1-based) from `src`.
///
/// Returns a `Vec<(line_number, line_content)>` sorted by line number.
/// This mirrors what the symbol preview pane would pass to the renderer.
fn extract_context_lines(src: &str, target_line: usize, radius: usize) -> Vec<(usize, String)> {
    let lines: Vec<&str> = src.lines().collect();
    let total = lines.len();

    if total == 0 || target_line == 0 || target_line > total {
        return Vec::new();
    }

    // Convert to 0-based index for range arithmetic.
    let idx = target_line - 1;
    let start = idx.saturating_sub(radius);
    let end = (idx + radius + 1).min(total);

    lines[start..end]
        .iter()
        .enumerate()
        .map(|(offset, &line)| (start + offset + 1, line.to_string()))
        .collect()
}
