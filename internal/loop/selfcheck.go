package loop

import "strings"

// selfCheckDirective nudges the agent to prove progress with a test rather than an
// opinion — a weak local model is a poor self-critic, but a self-generated test
// the acceptance verify then runs is a real check (THINK-004).
const selfCheckDirective = "Self-check: before marking progress, write or extend a test that captures this requirement, then make it pass. Prove it with a test — do not claim done on an opinion."

// SelfTestInPatch reports whether a unified-diff patch adds or edits a test file
// (the self-generated test of THINK-004). Language-agnostic: Go (_test.go),
// Python (test_*.py / *_test.py), JS/TS (*.test.*, *.spec.*), etc.
func SelfTestInPatch(patch string) bool {
	for _, line := range strings.Split(patch, "\n") {
		var path string
		switch {
		case strings.HasPrefix(line, "+++ "):
			path = strings.TrimPrefix(line, "+++ ")
		case strings.HasPrefix(line, "--- "):
			path = strings.TrimPrefix(line, "--- ")
		default:
			continue
		}
		if f := strings.Fields(strings.TrimSpace(path)); len(f) > 0 {
			path = f[0]
		}
		path = strings.TrimPrefix(path, "a/")
		path = strings.TrimPrefix(path, "b/")
		base := path[strings.LastIndex(path, "/")+1:]
		if strings.Contains(base, "_test.") || strings.Contains(base, ".test.") ||
			strings.Contains(base, ".spec.") || strings.HasPrefix(base, "test_") {
			return true
		}
	}
	return false
}
