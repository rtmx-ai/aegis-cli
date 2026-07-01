package opencode

import (
	"os"
	"strings"
)

// MemorySource is one project-memory input (CLAUDE.md / AGENTS.md / a skill), in
// precedence order: earlier sources are higher priority and included first.
type MemorySource struct {
	Name string // display label
	Path string // file to read
}

// DefaultMemoryBudget is the character budget for the assembled project memory,
// sized to leave room in the small model's ~32k window for the task itself.
const DefaultMemoryBudget = 12000

// AssembleMemory reads the sources in precedence order and concatenates them into a
// single prompt block, bounded by budget characters (MEM-003). Higher-precedence
// sources are included first; once the budget is reached the remaining (lower-
// precedence) sources are dropped whole — never truncated mid-file — so precedence
// stays clean. Missing or empty sources are skipped.
func AssembleMemory(sources []MemorySource, budget int) string {
	if budget <= 0 {
		budget = DefaultMemoryBudget
	}
	var b strings.Builder
	for _, src := range sources {
		data, err := os.ReadFile(src.Path)
		if err != nil || strings.TrimSpace(string(data)) == "" {
			continue
		}
		sep := ""
		if b.Len() > 0 {
			sep = "\n"
		}
		block := sep + "## " + src.Name + "\n\n" + string(data)
		if !strings.HasSuffix(block, "\n") {
			block += "\n"
		}
		if b.Len()+len(block) > budget {
			break // clean precedence: stop once the budget is reached
		}
		b.WriteString(block)
	}
	return b.String()
}
