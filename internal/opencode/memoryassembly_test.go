package opencode

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestMemoryAssembly → REQ-MEM-003: project memory is assembled in precedence order
// within a char budget — higher-precedence sources first, lower-precedence dropped
// whole when over budget, missing sources skipped.
func TestMemoryAssembly(t *testing.T) {
	dir := t.TempDir()
	write := func(name, content string) string {
		p := filepath.Join(dir, name)
		if err := os.WriteFile(p, []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
		return p
	}
	sources := []MemorySource{
		{Name: "AGENTS.md", Path: write("AGENTS.md", "agents guidance here")},
		{Name: "CLAUDE.md", Path: write("CLAUDE.md", "claude guidance here")},
		{Name: "skill", Path: write("skill.md", "a skill routine")},
	}

	// Ample budget: all three, in precedence order.
	full := AssembleMemory(sources, 10000)
	for _, w := range []string{"agents guidance", "claude guidance", "a skill routine"} {
		if !strings.Contains(full, w) {
			t.Errorf("assembly missing %q", w)
		}
	}
	if !(strings.Index(full, "agents") < strings.Index(full, "claude") && strings.Index(full, "claude") < strings.Index(full, "skill")) {
		t.Error("precedence order must be preserved (AGENTS > CLAUDE > skill)")
	}

	// Tight budget: only the highest-precedence source fits; lower dropped cleanly.
	tight := AssembleMemory(sources, 40)
	if !strings.Contains(tight, "agents guidance") {
		t.Error("highest-precedence source must be included")
	}
	if strings.Contains(tight, "a skill routine") {
		t.Error("lower-precedence source must be dropped whole when over budget")
	}

	// Missing source yields empty.
	if AssembleMemory([]MemorySource{{Name: "gone", Path: filepath.Join(dir, "nope.md")}}, 1000) != "" {
		t.Error("missing source must yield empty")
	}
}
