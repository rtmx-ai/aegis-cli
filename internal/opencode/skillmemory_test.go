package opencode

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestSkillMemory → REQ-MEM-002: human-curated skills load from a skills dir (each
// <name>/SKILL.md or top-level *.md) as MemorySources for context assembly; read-only
// (manual induction, no auto-learn).
func TestSkillMemory(t *testing.T) {
	dir := t.TempDir()
	if err := os.MkdirAll(filepath.Join(dir, "foo"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "foo", "SKILL.md"), []byte("how to foo"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "bar.md"), []byte("how to bar"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "notes.txt"), []byte("ignored"), 0o644); err != nil {
		t.Fatal(err)
	}

	skills := LoadSkills(dir)
	if len(skills) != 2 {
		t.Fatalf("want 2 skills (foo/SKILL.md + bar.md), got %d: %+v", len(skills), skills)
	}
	// Sorted by name for stable precedence.
	if skills[0].Name != "skill:bar" || skills[1].Name != "skill:foo" {
		t.Errorf("skills should be name-sorted; got %q, %q", skills[0].Name, skills[1].Name)
	}
	// Loadable content assembles into context.
	asm := AssembleMemory(skills, 10000)
	if !strings.Contains(asm, "how to foo") || !strings.Contains(asm, "how to bar") {
		t.Errorf("assembled skills missing content:\n%s", asm)
	}
	// A missing dir is empty (no crash).
	if LoadSkills(filepath.Join(dir, "nope")) != nil {
		t.Error("missing skills dir must yield nil")
	}
}
