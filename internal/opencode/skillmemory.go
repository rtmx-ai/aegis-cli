package opencode

import (
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// LoadSkills reads human-curated skill files from dir as MemorySources for context
// assembly (MEM-002): each subdir's SKILL.md, or a top-level *.md. Skills are
// distilled "how we did X" routines (the Anthropic Skills pattern); induction is
// MANUAL — aegis never auto-writes skills (auto-learn is out, cf. MEM-004), so this
// is read-only. Returns sources sorted by name for stable precedence.
func LoadSkills(dir string) []MemorySource {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil
	}
	var out []MemorySource
	for _, e := range entries {
		if e.IsDir() {
			p := filepath.Join(dir, e.Name(), "SKILL.md")
			if fileExists(p) {
				out = append(out, MemorySource{Name: "skill:" + e.Name(), Path: p})
			}
			continue
		}
		if strings.HasSuffix(e.Name(), ".md") {
			out = append(out, MemorySource{Name: "skill:" + strings.TrimSuffix(e.Name(), ".md"), Path: filepath.Join(dir, e.Name())})
		}
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Name < out[j].Name })
	return out
}
