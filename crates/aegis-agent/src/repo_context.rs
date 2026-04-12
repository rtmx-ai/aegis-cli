//! Live repository context gathered at session start.
//!
//! When the agent begins a session, it gathers context about the current
//! repository (git branch, status, recent commits, project type, directory
//! tree, and any custom system prompt file). This context is formatted as
//! a prompt section and injected into the system prompt so the LLM has
//! awareness of the project without the user needing to explain it.
//!
//! All operations are best-effort: if git is not installed, the directory
//! is not a repository, or any command fails, the corresponding field is
//! `None` or empty. Nothing in this module panics or returns an error.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Repository context gathered at session start for system prompt injection.
#[derive(Debug, Clone, Default)]
pub struct RepoContext {
    /// Current git branch name, e.g. "main".
    pub git_branch: Option<String>,
    /// Summary of working tree status, e.g. "3 modified, 1 untracked".
    pub git_status_summary: Option<String>,
    /// Last N commit one-line summaries (most recent first).
    pub recent_commits: Vec<String>,
    /// Detected project type based on manifest files.
    pub project_type: Option<String>,
    /// Two-level indented directory tree of the project root.
    pub directory_tree: String,
    /// Contents of `.aegis/system_prompt.md` if it exists.
    pub system_prompt_file: Option<String>,
}

/// Directories to skip when building the directory tree.
const SKIP_DIRS: &[&str] = &[
    "target",
    "node_modules",
    ".git",
    "dist",
    "build",
    "__pycache__",
];

/// Maximum entries in the directory tree output.
const MAX_TREE_ENTRIES: usize = 50;

impl RepoContext {
    /// Gather context from the given working directory. All operations are
    /// best-effort: failures produce `None` / empty values, never panics.
    pub fn gather(working_dir: &Path) -> Self {
        let git_branch = Self::read_git_branch(working_dir);
        let git_status_summary = Self::read_git_status_summary(working_dir);
        let recent_commits = Self::read_recent_commits(working_dir);
        let project_type = Self::detect_project_type(working_dir);
        let directory_tree = Self::build_directory_tree(working_dir);
        let system_prompt_file = Self::read_system_prompt_file(working_dir);

        Self {
            git_branch,
            git_status_summary,
            recent_commits,
            project_type,
            directory_tree,
            system_prompt_file,
        }
    }

    /// Format the gathered context as a string suitable for system prompt
    /// injection. Returns a section with markdown-style headers.
    pub fn to_prompt_section(&self) -> String {
        let mut out = String::new();
        out.push_str("# Repository Context\n\n");

        if let Some(ref branch) = self.git_branch {
            let _ = writeln!(out, "**Branch:** {branch}");
        }
        if let Some(ref status) = self.git_status_summary {
            let _ = writeln!(out, "**Working tree:** {status}");
        }
        if !self.recent_commits.is_empty() {
            out.push_str("\n## Recent commits\n");
            for c in &self.recent_commits {
                let _ = writeln!(out, "- {c}");
            }
        }
        if let Some(ref pt) = self.project_type {
            let _ = writeln!(out, "\n**Project type:** {pt}");
        }
        if !self.directory_tree.is_empty() {
            out.push_str("\n## Directory tree\n```\n");
            out.push_str(&self.directory_tree);
            out.push_str("```\n");
        }
        if let Some(ref prompt) = self.system_prompt_file {
            out.push_str("\n## Project system prompt\n");
            out.push_str(prompt);
            out.push('\n');
        }

        out
    }

    // -- Private helpers --

    fn run_git(working_dir: &Path, args: &[&str]) -> Option<String> {
        let output = Command::new("git")
            .args(args)
            .current_dir(working_dir)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if stdout.is_empty() {
            None
        } else {
            Some(stdout)
        }
    }

    fn read_git_branch(working_dir: &Path) -> Option<String> {
        Self::run_git(working_dir, &["rev-parse", "--abbrev-ref", "HEAD"])
    }

    fn read_git_status_summary(working_dir: &Path) -> Option<String> {
        // Run git status directly rather than via run_git, because
        // run_git returns None for empty stdout, but empty porcelain
        // output means "clean" which we want to report.
        let output = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(working_dir)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        if lines.is_empty() {
            return Some("clean".to_string());
        }

        let mut modified = 0u32;
        let mut added = 0u32;
        let mut deleted = 0u32;
        let mut untracked = 0u32;

        for line in &lines {
            let prefix = if line.len() >= 2 { &line[..2] } else { "  " };
            match prefix.trim() {
                "M" | "MM" | "AM" => modified += 1,
                "A" => added += 1,
                "D" => deleted += 1,
                "??" => untracked += 1,
                _ => modified += 1, // catch-all for renames, copies, etc.
            }
        }

        let mut parts = Vec::new();
        if modified > 0 {
            parts.push(format!("{modified} modified"));
        }
        if added > 0 {
            parts.push(format!("{added} added"));
        }
        if deleted > 0 {
            parts.push(format!("{deleted} deleted"));
        }
        if untracked > 0 {
            parts.push(format!("{untracked} untracked"));
        }
        Some(parts.join(", "))
    }

    fn read_recent_commits(working_dir: &Path) -> Vec<String> {
        Self::run_git(working_dir, &["log", "--oneline", "-5"])
            .map(|s| s.lines().map(String::from).collect())
            .unwrap_or_default()
    }

    fn detect_project_type(working_dir: &Path) -> Option<String> {
        let checks: &[(&str, &str)] = &[
            ("Cargo.toml", "rust"),
            ("package.json", "node"),
            ("go.mod", "go"),
            ("pyproject.toml", "python"),
            ("setup.py", "python"),
            ("pom.xml", "java"),
            ("build.gradle", "java"),
            ("CMakeLists.txt", "cmake"),
            ("Makefile", "make"),
        ];
        for (file, kind) in checks {
            if working_dir.join(file).exists() {
                return Some((*kind).to_string());
            }
        }
        None
    }

    fn build_directory_tree(working_dir: &Path) -> String {
        let mut out = String::new();
        let mut count = 0usize;
        let top_entries = Self::sorted_dir_entries(working_dir);

        for entry in &top_entries {
            if count >= MAX_TREE_ENTRIES {
                break;
            }
            let name = Self::file_name_str(entry);
            if Self::is_hidden(&name) {
                continue;
            }
            if entry.is_dir() {
                if SKIP_DIRS.contains(&name.as_str()) {
                    continue;
                }
                let _ = writeln!(out, "{name}/");
                count += 1;
                // Second level
                let children = Self::sorted_dir_entries(entry);
                for child in &children {
                    if count >= MAX_TREE_ENTRIES {
                        break;
                    }
                    let cname = Self::file_name_str(child);
                    if Self::is_hidden(&cname) {
                        continue;
                    }
                    if child.is_dir() {
                        let _ = writeln!(out, "  {cname}/");
                    } else {
                        let _ = writeln!(out, "  {cname}");
                    }
                    count += 1;
                }
            } else {
                let _ = writeln!(out, "{name}");
                count += 1;
            }
        }
        out
    }

    fn sorted_dir_entries(dir: &Path) -> Vec<PathBuf> {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut entries: BTreeSet<PathBuf> = BTreeSet::new();
        for entry in rd.flatten() {
            entries.insert(entry.path());
        }
        entries.into_iter().collect()
    }

    fn file_name_str(path: &Path) -> String {
        path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    }

    fn is_hidden(name: &str) -> bool {
        name.starts_with('.')
    }

    fn read_system_prompt_file(working_dir: &Path) -> Option<String> {
        let path = working_dir.join(".aegis").join("system_prompt.md");
        std::fs::read_to_string(path).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    fn make_tempdir() -> TempDir {
        TempDir::new().expect("failed to create tempdir")
    }

    fn git_init(dir: &Path) {
        Command::new("git")
            .args(["init"])
            .current_dir(dir)
            .output()
            .expect("git init failed");
        // Configure user for commits
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir)
            .output()
            .ok();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(dir)
            .output()
            .ok();
    }

    // @req REQ-AGENT-026
    #[test]
    fn gather_non_git_dir_returns_none_for_git_fields() {
        let tmp = make_tempdir();
        let ctx = RepoContext::gather(tmp.path());
        assert!(ctx.git_branch.is_none());
        assert!(ctx.git_status_summary.is_none());
        assert!(ctx.recent_commits.is_empty());
    }

    // @req REQ-AGENT-026
    #[test]
    fn gather_git_dir_returns_branch_name() {
        let tmp = make_tempdir();
        git_init(tmp.path());
        // Create an initial commit so HEAD exists
        fs::write(tmp.path().join("README"), "hello").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(tmp.path())
            .output()
            .unwrap();

        let ctx = RepoContext::gather(tmp.path());
        let branch = ctx.git_branch.expect("should have branch");
        // Default branch could be "main" or "master" depending on git config.
        assert!(
            branch == "main" || branch == "master",
            "unexpected branch: {branch}"
        );
    }

    // @req REQ-AGENT-026
    #[test]
    fn project_type_detects_cargo_toml_as_rust() {
        let tmp = make_tempdir();
        fs::write(tmp.path().join("Cargo.toml"), "[package]").unwrap();
        let ctx = RepoContext::gather(tmp.path());
        assert_eq!(ctx.project_type.as_deref(), Some("rust"));
    }

    // @req REQ-AGENT-026
    #[test]
    fn project_type_detects_package_json_as_node() {
        let tmp = make_tempdir();
        fs::write(tmp.path().join("package.json"), "{}").unwrap();
        let ctx = RepoContext::gather(tmp.path());
        assert_eq!(ctx.project_type.as_deref(), Some("node"));
    }

    // @req REQ-AGENT-026
    #[test]
    fn project_type_detects_go_mod_as_go() {
        let tmp = make_tempdir();
        fs::write(tmp.path().join("go.mod"), "module foo").unwrap();
        let ctx = RepoContext::gather(tmp.path());
        assert_eq!(ctx.project_type.as_deref(), Some("go"));
    }

    // @req REQ-AGENT-026
    #[test]
    fn project_type_detects_pyproject_toml_as_python() {
        let tmp = make_tempdir();
        fs::write(tmp.path().join("pyproject.toml"), "[tool]").unwrap();
        let ctx = RepoContext::gather(tmp.path());
        assert_eq!(ctx.project_type.as_deref(), Some("python"));
    }

    // @req REQ-AGENT-026
    #[test]
    fn project_type_returns_none_for_unknown() {
        let tmp = make_tempdir();
        let ctx = RepoContext::gather(tmp.path());
        assert!(ctx.project_type.is_none());
    }

    // @req REQ-AGENT-026
    #[test]
    fn directory_tree_includes_files_but_skips_hidden() {
        let tmp = make_tempdir();
        fs::write(tmp.path().join("visible.txt"), "").unwrap();
        fs::write(tmp.path().join(".hidden"), "").unwrap();
        fs::create_dir(tmp.path().join(".git")).unwrap();
        fs::create_dir(tmp.path().join("src")).unwrap();
        fs::write(tmp.path().join("src").join("main.rs"), "").unwrap();

        let ctx = RepoContext::gather(tmp.path());
        assert!(
            ctx.directory_tree.contains("visible.txt"),
            "tree should contain visible.txt: {}",
            ctx.directory_tree
        );
        assert!(
            !ctx.directory_tree.contains(".hidden"),
            "tree should not contain .hidden: {}",
            ctx.directory_tree
        );
        assert!(
            !ctx.directory_tree.contains(".git"),
            "tree should not contain .git: {}",
            ctx.directory_tree
        );
        assert!(
            ctx.directory_tree.contains("src/"),
            "tree should contain src/: {}",
            ctx.directory_tree
        );
        assert!(
            ctx.directory_tree.contains("  main.rs"),
            "tree should contain indented main.rs: {}",
            ctx.directory_tree
        );
    }

    // @req REQ-AGENT-026
    #[test]
    fn directory_tree_skips_target_and_node_modules() {
        let tmp = make_tempdir();
        fs::create_dir(tmp.path().join("target")).unwrap();
        fs::create_dir(tmp.path().join("node_modules")).unwrap();
        fs::create_dir(tmp.path().join("src")).unwrap();

        let ctx = RepoContext::gather(tmp.path());
        assert!(!ctx.directory_tree.contains("target"));
        assert!(!ctx.directory_tree.contains("node_modules"));
        assert!(ctx.directory_tree.contains("src/"));
    }

    // @req REQ-AGENT-026
    #[test]
    fn to_prompt_section_produces_nonempty_string_with_headers() {
        let tmp = make_tempdir();
        fs::write(tmp.path().join("Cargo.toml"), "[package]").unwrap();
        fs::create_dir(tmp.path().join("src")).unwrap();

        let ctx = RepoContext::gather(tmp.path());
        let section = ctx.to_prompt_section();
        assert!(
            section.contains("# Repository Context"),
            "should have header"
        );
        assert!(
            section.contains("**Project type:** rust"),
            "should mention project type"
        );
        assert!(
            section.contains("## Directory tree"),
            "should have directory tree section"
        );
        assert!(!section.is_empty());
    }

    // @req REQ-AGENT-026
    #[test]
    fn system_prompt_file_returns_contents_when_exists() {
        let tmp = make_tempdir();
        let aegis_dir = tmp.path().join(".aegis");
        fs::create_dir(&aegis_dir).unwrap();
        fs::write(
            aegis_dir.join("system_prompt.md"),
            "You are a helpful assistant.",
        )
        .unwrap();

        let ctx = RepoContext::gather(tmp.path());
        assert_eq!(
            ctx.system_prompt_file.as_deref(),
            Some("You are a helpful assistant.")
        );
    }

    // @req REQ-AGENT-026
    #[test]
    fn system_prompt_file_returns_none_when_missing() {
        let tmp = make_tempdir();
        let ctx = RepoContext::gather(tmp.path());
        assert!(ctx.system_prompt_file.is_none());
    }

    // @req REQ-AGENT-026
    #[test]
    fn git_status_summary_reports_clean_on_clean_repo() {
        let tmp = make_tempdir();
        git_init(tmp.path());
        fs::write(tmp.path().join("file.txt"), "hello").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(tmp.path())
            .output()
            .unwrap();

        let ctx = RepoContext::gather(tmp.path());
        assert_eq!(ctx.git_status_summary.as_deref(), Some("clean"));
    }

    // @req REQ-AGENT-026
    #[test]
    fn git_status_summary_counts_modified_and_untracked() {
        let tmp = make_tempdir();
        git_init(tmp.path());
        fs::write(tmp.path().join("tracked.txt"), "hello").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        // Modify tracked file and add untracked file
        fs::write(tmp.path().join("tracked.txt"), "changed").unwrap();
        fs::write(tmp.path().join("new_file.txt"), "new").unwrap();

        let ctx = RepoContext::gather(tmp.path());
        let summary = ctx.git_status_summary.expect("should have status");
        assert!(
            summary.contains("modified"),
            "should mention modified: {summary}"
        );
        assert!(
            summary.contains("untracked"),
            "should mention untracked: {summary}"
        );
    }

    // @req REQ-AGENT-026
    #[test]
    fn recent_commits_returns_commit_lines() {
        let tmp = make_tempdir();
        git_init(tmp.path());
        for i in 0..3 {
            fs::write(
                tmp.path().join(format!("file{i}.txt")),
                format!("content {i}"),
            )
            .unwrap();
            Command::new("git")
                .args(["add", "."])
                .current_dir(tmp.path())
                .output()
                .unwrap();
            Command::new("git")
                .args(["commit", "-m", &format!("commit {i}")])
                .current_dir(tmp.path())
                .output()
                .unwrap();
        }

        let ctx = RepoContext::gather(tmp.path());
        assert_eq!(ctx.recent_commits.len(), 3);
        assert!(ctx.recent_commits[0].contains("commit 2"));
        assert!(ctx.recent_commits[2].contains("commit 0"));
    }

    // @req REQ-AGENT-026
    #[test]
    fn to_prompt_section_includes_system_prompt_file_content() {
        let tmp = make_tempdir();
        let aegis_dir = tmp.path().join(".aegis");
        fs::create_dir(&aegis_dir).unwrap();
        fs::write(
            aegis_dir.join("system_prompt.md"),
            "Custom project instructions.",
        )
        .unwrap();

        let ctx = RepoContext::gather(tmp.path());
        let section = ctx.to_prompt_section();
        assert!(section.contains("## Project system prompt"));
        assert!(section.contains("Custom project instructions."));
    }

    // @req REQ-AGENT-026
    #[test]
    fn directory_tree_max_entries_capped() {
        let tmp = make_tempdir();
        // Create more than MAX_TREE_ENTRIES files
        for i in 0..60 {
            fs::write(tmp.path().join(format!("file_{i:03}.txt")), "").unwrap();
        }

        let ctx = RepoContext::gather(tmp.path());
        let line_count = ctx.directory_tree.lines().count();
        assert!(
            line_count <= MAX_TREE_ENTRIES,
            "tree should be capped at {MAX_TREE_ENTRIES} entries, got {line_count}"
        );
    }
}
