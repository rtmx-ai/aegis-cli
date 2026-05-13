//! Regex-free symbol extractor for common programming languages.
//!
//! Extracts top-level declarations (functions, structs, enums, classes, etc.)
//! from source files using line-by-line pattern matching. Detects the language
//! from the file extension.

/// Kind of symbol extracted from source code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Struct,
    Enum,
    Impl,
    Class,
    Type,
    Trait,
}

/// A symbol extracted from a source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    /// The symbol name (e.g., "main", "MyStruct").
    pub name: String,
    /// The kind of symbol.
    pub kind: SymbolKind,
    /// 1-based line number where the symbol was found.
    pub line: usize,
    /// Path of the file the symbol was extracted from.
    pub file_path: String,
}

/// Extract symbols from `content`, using the file extension in `file_path`
/// to select language-specific patterns.
///
/// Returns an empty `Vec` for unrecognised extensions.
pub fn extract_symbols(content: &str, file_path: &str) -> Vec<Symbol> {
    let ext = file_path.rsplit('.').next().unwrap_or("").to_lowercase();

    let matchers: &[(&str, SymbolKind)] = match ext.as_str() {
        "rs" => &[
            ("fn ", SymbolKind::Function),
            ("struct ", SymbolKind::Struct),
            ("enum ", SymbolKind::Enum),
            ("impl ", SymbolKind::Impl),
            ("trait ", SymbolKind::Trait),
        ],
        "py" => &[
            ("def ", SymbolKind::Function),
            ("class ", SymbolKind::Class),
        ],
        "go" => &[("func ", SymbolKind::Function), ("type ", SymbolKind::Type)],
        "js" | "ts" => &[
            ("function ", SymbolKind::Function),
            ("class ", SymbolKind::Class),
        ],
        _ => return Vec::new(),
    };

    let mut symbols = Vec::new();

    for (line_no, line) in content.lines().enumerate() {
        let trimmed = line.trim_start();

        // JS/TS arrow functions: `const name = ... =>`
        if (ext == "js" || ext == "ts") && trimmed.starts_with("const ") && line.contains("=>") {
            if let Some(name) = extract_identifier(trimmed, "const ") {
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Function,
                    line: line_no + 1,
                    file_path: file_path.to_string(),
                });
            }
            continue;
        }

        for (keyword, kind) in matchers {
            let after_kw = if let Some(rest) = trimmed.strip_prefix("pub ") {
                if rest.starts_with(*keyword) {
                    rest
                } else {
                    continue;
                }
            } else if trimmed.starts_with(keyword) {
                trimmed
            } else {
                continue;
            };
            if let Some(name) = extract_identifier(after_kw, keyword) {
                symbols.push(Symbol {
                    name,
                    kind: kind.clone(),
                    line: line_no + 1,
                    file_path: file_path.to_string(),
                });
            }
            break;
        }
    }

    symbols
}

/// Given a line starting with `keyword`, extract the first identifier that
/// follows it.  An identifier is a run of `[A-Za-z0-9_]`.
fn extract_identifier(line: &str, keyword: &str) -> Option<String> {
    let rest = line.strip_prefix(keyword)?;
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() { None } else { Some(name) }
}

/// A location where a symbol was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolLocation {
    pub file_path: String,
    pub line: usize,
    pub kind: SymbolKind,
}

/// Index mapping symbol names to their locations across multiple files.
#[derive(Debug, Default)]
pub struct SymbolIndex {
    entries: std::collections::HashMap<String, Vec<SymbolLocation>>,
}

impl SymbolIndex {
    /// Create a new empty index.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add all symbols from a file to the index.
    pub fn index_file(&mut self, content: &str, file_path: &str) {
        let symbols = extract_symbols(content, file_path);
        for sym in symbols {
            self.entries
                .entry(sym.name)
                .or_default()
                .push(SymbolLocation {
                    file_path: sym.file_path,
                    line: sym.line,
                    kind: sym.kind,
                });
        }
    }

    /// Look up all locations for a given symbol name (exact match).
    pub fn lookup(&self, name: &str) -> &[SymbolLocation] {
        self.entries.get(name).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Search for symbols matching a prefix (case-insensitive).
    pub fn search(&self, query: &str) -> Vec<(&str, &[SymbolLocation])> {
        let query_lower = query.to_lowercase();
        let mut results: Vec<(&str, &[SymbolLocation])> = self
            .entries
            .iter()
            .filter(|(name, _)| name.to_lowercase().contains(&query_lower))
            .map(|(name, locs)| (name.as_str(), locs.as_slice()))
            .collect();
        results.sort_by_key(|(name, _)| name.to_lowercase());
        results
    }

    /// Total number of unique symbol names in the index.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-TUI-083
    #[test]
    fn test_symbol_extractor_finds_rust_fn() {
        let src = r#"
pub fn main() {
    println!("hello");
}

struct Config {
    name: String,
}

pub enum Mode {
    Debug,
    Release,
}

impl Config {
    fn new() -> Self { todo!() }
}

trait Renderable {
    fn render(&self);
}
"#;
        let symbols = extract_symbols(src, "src/main.rs");
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        let kinds: Vec<&SymbolKind> = symbols.iter().map(|s| &s.kind).collect();

        assert!(names.contains(&"main"), "should find fn main");
        assert!(names.contains(&"Config"), "should find struct Config");
        assert!(names.contains(&"Mode"), "should find enum Mode");
        assert!(names.contains(&"new"), "should find fn new inside impl");
        assert!(
            names.contains(&"Renderable"),
            "should find trait Renderable"
        );

        assert!(kinds.contains(&&SymbolKind::Function));
        assert!(kinds.contains(&&SymbolKind::Struct));
        assert!(kinds.contains(&&SymbolKind::Enum));

        // Verify line numbers are 1-based and sensible.
        for s in &symbols {
            assert!(s.line > 0, "line numbers must be 1-based");
            assert_eq!(s.file_path, "src/main.rs");
        }
    }

    // rtmx:req REQ-TUI-083
    #[test]
    fn test_symbol_extractor_finds_python() {
        let src = r#"
def hello():
    pass

class MyClass:
    def method(self):
        pass
"#;
        let symbols = extract_symbols(src, "app.py");
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();

        assert!(names.contains(&"hello"), "should find def hello");
        assert!(names.contains(&"MyClass"), "should find class MyClass");
        assert!(names.contains(&"method"), "should find def method");
    }

    // rtmx:req REQ-TUI-083
    #[test]
    fn test_symbol_extractor_finds_go() {
        let src = r#"
func main() {
    fmt.Println("hello")
}

type Config struct {
    Name string
}

func (c *Config) String() string {
    return c.Name
}
"#;
        let symbols = extract_symbols(src, "main.go");
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();

        assert!(names.contains(&"main"), "should find func main");
        assert!(names.contains(&"Config"), "should find type Config");
    }

    // rtmx:req REQ-TUI-083
    #[test]
    fn test_symbol_extractor_finds_js() {
        let src = r#"
function greet(name) {
    return "hello " + name;
}

class App {
    constructor() {}
}

const add = (a, b) => a + b;
"#;
        let symbols = extract_symbols(src, "index.js");
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();

        assert!(names.contains(&"greet"), "should find function greet");
        assert!(names.contains(&"App"), "should find class App");
        assert!(names.contains(&"add"), "should find const arrow fn add");
    }

    // rtmx:req REQ-TUI-083
    #[test]
    fn test_symbol_extractor_unknown_ext_returns_empty() {
        let symbols = extract_symbols("some content", "data.csv");
        assert!(
            symbols.is_empty(),
            "unknown extension should return empty Vec"
        );
    }

    // rtmx:req REQ-TUI-084
    #[test]
    fn test_symbol_index_lookup() {
        let mut index = SymbolIndex::new();
        assert!(index.is_empty());

        let rust_src = "pub fn main() {}\nstruct Config {}\nfn helper() {}";
        index.index_file(rust_src, "src/main.rs");

        assert!(!index.is_empty());
        assert!(index.len() >= 3);

        // Exact lookup
        let main_locs = index.lookup("main");
        assert_eq!(main_locs.len(), 1);
        assert_eq!(main_locs[0].file_path, "src/main.rs");
        assert_eq!(main_locs[0].line, 1);
        assert_eq!(main_locs[0].kind, SymbolKind::Function);

        let config_locs = index.lookup("Config");
        assert_eq!(config_locs.len(), 1);
        assert_eq!(config_locs[0].kind, SymbolKind::Struct);

        // Missing symbol
        assert!(index.lookup("nonexistent").is_empty());

        // Search by prefix
        let results = index.search("con");
        assert!(
            results.iter().any(|(name, _)| *name == "Config"),
            "search for 'con' must find Config"
        );

        // Multiple files
        let py_src = "def main():\n    pass\nclass Config:\n    pass";
        index.index_file(py_src, "app.py");
        let main_locs = index.lookup("main");
        assert_eq!(
            main_locs.len(),
            2,
            "main should appear in both Rust and Python files"
        );
    }
}
