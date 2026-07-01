package index

import (
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// AssembleContext builds a bounded context bundle for a task (INDEX-005): the
// repo-map skeleton (breadth) followed by the bodies of the files relevant to the
// task's mentions, ranked to fit a char budget — repo map first (half the budget),
// then file bodies until the budget is spent. Model-free and grep-first (INDEX-003);
// SCIP-precise def selection (INDEX-002) is an optional future enhancement to which
// files are pulled, not a prerequisite.
func AssembleContext(root string, mentions []string, budget int) string {
	if budget <= 0 {
		budget = 8000
	}
	var b strings.Builder
	if repoMap, err := Build(Options{Root: root, Mentions: mentions, TokenBudget: budget / 2}); err == nil && repoMap != "" {
		b.WriteString("## Repo map\n\n")
		b.WriteString(repoMap)
		b.WriteString("\n")
	}

	files, _ := goFiles(root)
	sort.Strings(files)
	for _, rel := range files {
		data, err := os.ReadFile(filepath.Join(root, rel))
		if err != nil || !contentMatchesAny(string(data), mentions) {
			continue
		}
		block := "\n## " + rel + "\n\n```\n" + string(data) + "```\n"
		if b.Len()+len(block) > budget {
			break // budget spent: stop rather than truncate a file mid-body
		}
		b.WriteString(block)
	}
	return b.String()
}

func contentMatchesAny(content string, mentions []string) bool {
	for _, m := range mentions {
		if m != "" && strings.Contains(content, m) {
			return true
		}
	}
	return false
}
