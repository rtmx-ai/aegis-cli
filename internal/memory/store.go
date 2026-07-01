// Package memory is aegis's working-memory store (MEM-005): a queryable,
// size-bounded, machine-written scratch store of facts and code snippets the agent
// emits during a task. Per-kind count caps and oldest-first GC keep it from
// overrunning the small model's 32k window; it persists to a file so it survives
// resume. It is deliberately SEPARATE from human-authored intent (CLAUDE.md /
// AGENTS.md), which is never touched here.
//
// The RA.Aid reference design uses SQLite (KeyFact/KeySnippet + GC agents); aegis
// uses a pure-Go file backend to keep the single static binary CGO- and
// dependency-free. A SQLite/FTS backend is a future upgrade if query complexity
// grows (see INDEX-005).
package memory

import (
	"encoding/json"
	"os"
	"sort"
	"strings"
)

// Kind distinguishes a curated fact from a code snippet.
type Kind string

const (
	Fact    Kind = "fact"
	Snippet Kind = "snippet"
)

// Default per-kind caps (RA.Aid-derived), bounding the store's context footprint.
const (
	DefaultMaxFacts    = 50
	DefaultMaxSnippets = 35
)

// Entry is one machine-written memory item.
type Entry struct {
	Kind Kind   `json:"kind"`
	Key  string `json:"key"`
	Text string `json:"text"`
	Seq  int    `json:"seq"` // monotonic insertion order, for oldest-first GC
}

// Store is the working-memory store, backed by a JSON file at path.
type Store struct {
	path    string
	maxFact int
	maxSnip int
	seq     int
	entries []Entry
}

// Open loads (or creates) a store at path with per-kind count caps (0 -> defaults).
func Open(path string, maxFact, maxSnip int) (*Store, error) {
	if maxFact <= 0 {
		maxFact = DefaultMaxFacts
	}
	if maxSnip <= 0 {
		maxSnip = DefaultMaxSnippets
	}
	s := &Store{path: path, maxFact: maxFact, maxSnip: maxSnip}
	if b, err := os.ReadFile(path); err == nil {
		_ = json.Unmarshal(b, &s.entries)
		for _, e := range s.entries {
			if e.Seq > s.seq {
				s.seq = e.Seq
			}
		}
	}
	return s, nil
}

// Emit adds a fact/snippet, deduped by (kind,key): an existing key is updated in
// place, otherwise appended. The kind is then GC'd to its cap (oldest evicted) and
// the store persisted.
func (s *Store) Emit(kind Kind, key, text string) error {
	for i := range s.entries {
		if s.entries[i].Kind == kind && s.entries[i].Key == key {
			s.seq++
			s.entries[i].Text = text
			s.entries[i].Seq = s.seq
			return s.gcAndSave(kind)
		}
	}
	s.seq++
	s.entries = append(s.entries, Entry{Kind: kind, Key: key, Text: text, Seq: s.seq})
	return s.gcAndSave(kind)
}

func (s *Store) capFor(kind Kind) int {
	if kind == Snippet {
		return s.maxSnip
	}
	return s.maxFact
}

func (s *Store) gcAndSave(kind Kind) error {
	var idx []int
	for i, e := range s.entries {
		if e.Kind == kind {
			idx = append(idx, i)
		}
	}
	if over := len(idx) - s.capFor(kind); over > 0 {
		sort.Slice(idx, func(a, b int) bool { return s.entries[idx[a]].Seq < s.entries[idx[b]].Seq })
		drop := map[int]bool{}
		for _, i := range idx[:over] {
			drop[i] = true
		}
		kept := make([]Entry, 0, len(s.entries)-over)
		for i, e := range s.entries {
			if !drop[i] {
				kept = append(kept, e)
			}
		}
		s.entries = kept
	}
	b, err := json.Marshal(s.entries)
	if err != nil {
		return err
	}
	return os.WriteFile(s.path, b, 0o644)
}

// Query returns entries whose key or text contains substr (case-insensitive).
func (s *Store) Query(substr string) []Entry {
	q := strings.ToLower(substr)
	var out []Entry
	for _, e := range s.entries {
		if strings.Contains(strings.ToLower(e.Key), q) || strings.Contains(strings.ToLower(e.Text), q) {
			out = append(out, e)
		}
	}
	return out
}

// All returns a copy of the current entries.
func (s *Store) All() []Entry {
	out := make([]Entry, len(s.entries))
	copy(out, s.entries)
	return out
}

// Render returns markdown for injection into the model context ("" when empty).
func (s *Store) Render() string {
	if len(s.entries) == 0 {
		return ""
	}
	var b strings.Builder
	b.WriteString("# Working memory (machine-written; facts + snippets)\n\n")
	for _, e := range s.entries {
		b.WriteString("- [" + string(e.Kind) + "] " + e.Key + ": " + e.Text + "\n")
	}
	return b.String()
}
