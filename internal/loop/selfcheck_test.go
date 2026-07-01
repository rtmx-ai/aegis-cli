package loop

import (
	"strings"
	"testing"
)

// TestSelfGeneratedCheck → REQ-THINK-004: the loop recognizes a self-generated test
// in the agent's diff and, when RequireSelfTest is set, injects the self-check
// directive until the agent writes one — proving progress with a test, not opinion.
func TestSelfGeneratedCheck(t *testing.T) {
	// The detector recognizes test files across languages, and ignores non-tests.
	for _, p := range []string{"+++ b/foo_test.go", "--- a/pkg/bar_test.go", "+++ b/test_thing.py", "+++ b/x.test.js", "+++ b/y.spec.ts"} {
		if !SelfTestInPatch(p) {
			t.Errorf("should detect a test file: %q", p)
		}
	}
	for _, p := range []string{"+++ b/foo.go", "+++ b/latest_data.go", "--- a/README.md", ""} {
		if SelfTestInPatch(p) {
			t.Errorf("should NOT flag a non-test: %q", p)
		}
	}

	// Loop wiring: with RequireSelfTest, the self-check directive is injected into
	// the drive context (the fake harness never writes a test).
	rt := rtmxWithPassing("A-001")
	h := &fbHarness{Fake: harnessFake()}
	l, err := New(testCfg(), Deps{RTMX: rt, Harness: h, RequireSelfTest: true})
	if err != nil {
		t.Fatal(err)
	}
	if _, err := l.Run(ctx(), true); err != nil {
		t.Fatalf("run: %v", err)
	}
	if len(h.feedbacks) == 0 || !strings.Contains(h.feedbacks[0], "write or extend a test") {
		t.Errorf("RequireSelfTest must inject the self-check directive; got %v", h.feedbacks)
	}
}
