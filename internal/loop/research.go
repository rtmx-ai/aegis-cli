package loop

import (
	"os"
	"path/filepath"
	"strconv"
	"strings"

	"github.com/rtmx-ai/aegis-cli/internal/memory"
	"github.com/rtmx-ai/aegis-cli/internal/rtmx"
)

var sourceExts = map[string]bool{
	".go": true, ".py": true, ".js": true, ".ts": true, ".tsx": true, ".jsx": true,
	".java": true, ".c": true, ".cc": true, ".cpp": true, ".h": true, ".hpp": true,
	".rs": true, ".rb": true, ".php": true, ".cs": true, ".md": true, ".txt": true,
	".yaml": true, ".yml": true, ".toml": true, ".json": true,
}

func isSourceFile(path string) bool { return sourceExts[strings.ToLower(filepath.Ext(path))] }

// ResearchPreStage does a bounded, model-free discovery pass (RA.Aid stage 1,
// LONGRUN-011): it greps the task's key terms over root and emits the matching
// file:line snippets into the working-memory store, so planning starts from
// curated context rather than raw file dumps. Returns the number of hits emitted,
// bounded by maxHits.
func ResearchPreStage(store *memory.Store, root string, terms []string, maxHits int) int {
	if store == nil || len(terms) == 0 {
		return 0
	}
	if maxHits <= 0 {
		maxHits = 20
	}
	hits := 0
	_ = filepath.WalkDir(root, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return nil
		}
		if hits >= maxHits {
			return filepath.SkipAll
		}
		if d.IsDir() {
			b := d.Name()
			if b == "vendor" || b == ".git" || b == "node_modules" || b == "testdata" || (strings.HasPrefix(b, ".") && b != ".") {
				return filepath.SkipDir
			}
			return nil
		}
		if !isSourceFile(path) {
			return nil
		}
		data, e := os.ReadFile(path)
		if e != nil {
			return nil
		}
		rel, _ := filepath.Rel(root, path)
		for i, line := range strings.Split(string(data), "\n") {
			if hits >= maxHits {
				break
			}
			for _, term := range terms {
				if term != "" && strings.Contains(line, term) {
					_ = store.Emit(memory.Snippet, rel+":"+strconv.Itoa(i+1), strings.TrimSpace(line))
					hits++
					break
				}
			}
		}
		return nil
	})
	return hits
}

// termsFromRequirement extracts capitalized identifiers (length >= 4) from a
// requirement's title as the discovery terms (LONGRUN-011).
func termsFromRequirement(req *rtmx.Requirement) []string {
	var terms []string
	seen := map[string]bool{}
	words := strings.FieldsFunc(req.Title, func(r rune) bool {
		return !((r >= 'a' && r <= 'z') || (r >= 'A' && r <= 'Z') || (r >= '0' && r <= '9') || r == '_')
	})
	for _, w := range words {
		if len(w) >= 4 && w[0] >= 'A' && w[0] <= 'Z' && !seen[w] {
			seen[w] = true
			terms = append(terms, w)
		}
	}
	return terms
}
