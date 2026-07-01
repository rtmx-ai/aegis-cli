package memory

import (
	"path/filepath"
	"testing"
)

// TestWorkingMemoryStore → REQ-MEM-005: a queryable, size-bounded, machine-written
// scratch store — per-kind count caps evict the oldest, dedupe updates in place,
// snippets and facts have independent caps, and the store survives reload (resume).
func TestWorkingMemoryStore(t *testing.T) {
	path := filepath.Join(t.TempDir(), "mem.json")
	s, err := Open(path, 2, 2) // caps: 2 facts, 2 snippets
	if err != nil {
		t.Fatal(err)
	}
	// Emit 3 facts -> capped at 2 (oldest evicted).
	for _, kv := range [][2]string{{"f1", "one"}, {"f2", "two"}, {"f3", "three"}} {
		if err := s.Emit(Fact, kv[0], kv[1]); err != nil {
			t.Fatal(err)
		}
	}
	if n := countKind(s, Fact); n != 2 {
		t.Fatalf("fact cap: want 2, got %d", n)
	}
	if len(s.Query("one")) != 0 {
		t.Error("oldest fact (f1) should have been evicted")
	}
	if len(s.Query("three")) != 1 {
		t.Error("newest fact (f3) should be present")
	}

	// Dedupe by key: update in place, count unchanged.
	if err := s.Emit(Fact, "f3", "three-updated"); err != nil {
		t.Fatal(err)
	}
	if len(s.Query("three-updated")) != 1 || countKind(s, Fact) != 2 {
		t.Errorf("dedupe/update must not grow the count; facts=%d", countKind(s, Fact))
	}

	// Snippets are an independent kind with their own cap.
	if err := s.Emit(Snippet, "s1", "code excerpt"); err != nil {
		t.Fatal(err)
	}

	// Survives reload (resume).
	s2, err := Open(path, 2, 2)
	if err != nil {
		t.Fatal(err)
	}
	if len(s2.Query("code excerpt")) != 1 {
		t.Error("snippet did not survive reload")
	}
	if s2.Render() == "" {
		t.Error("render should be non-empty after emits")
	}
}

func countKind(s *Store, k Kind) int {
	n := 0
	for _, e := range s.All() {
		if e.Kind == k {
			n++
		}
	}
	return n
}
