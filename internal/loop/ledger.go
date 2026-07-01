package loop

import (
	"os"
	"path/filepath"
	"strings"
)

// Ledger is an on-disk sub-task checklist for one requirement (LONGRUN-003): it is
// seeded from the requirement, re-read and re-injected each turn, and survives
// context compaction and process resume because it lives on disk, not in the
// model's window. It anchors a small local model on a long task.
type Ledger struct {
	Dir string // directory holding one markdown ledger per requirement
}

// Item is one checklist entry.
type Item struct {
	Done bool
	Text string
}

func (l Ledger) path(reqID string) string {
	return filepath.Join(l.Dir, strings.ReplaceAll(reqID, "/", "_")+".md")
}

// Seed creates the ledger for reqID from its title if it does not already exist —
// idempotent, so a resume keeps the existing checklist and its progress.
func (l Ledger) Seed(reqID, title string) error {
	p := l.path(reqID)
	if _, err := os.Stat(p); err == nil {
		return nil
	}
	if err := os.MkdirAll(l.Dir, 0o755); err != nil {
		return err
	}
	header := "# " + title + " (" + reqID + ")\n\n" +
		"- [ ] Understand the requirement and its linked test\n" +
		"- [ ] Implement the change\n" +
		"- [ ] Make the linked test pass\n"
	return os.WriteFile(p, []byte(header), 0o644)
}

// Items parses the checklist entries.
func (l Ledger) Items(reqID string) ([]Item, error) {
	b, err := os.ReadFile(l.path(reqID))
	if err != nil {
		return nil, err
	}
	var items []Item
	for _, line := range strings.Split(string(b), "\n") {
		t := strings.TrimSpace(line)
		switch {
		case strings.HasPrefix(t, "- [ ] "):
			items = append(items, Item{Done: false, Text: strings.TrimPrefix(t, "- [ ] ")})
		case strings.HasPrefix(t, "- [x] "):
			items = append(items, Item{Done: true, Text: strings.TrimPrefix(t, "- [x] ")})
		}
	}
	return items, nil
}

// Add appends an open checklist item.
func (l Ledger) Add(reqID, text string) error {
	f, err := os.OpenFile(l.path(reqID), os.O_APPEND|os.O_WRONLY, 0o644)
	if err != nil {
		return err
	}
	defer f.Close()
	_, err = f.WriteString("- [ ] " + text + "\n")
	return err
}

// Check marks the idx-th checklist item done and rewrites the ledger.
func (l Ledger) Check(reqID string, idx int) error {
	b, err := os.ReadFile(l.path(reqID))
	if err != nil {
		return err
	}
	lines := strings.Split(string(b), "\n")
	seen := -1
	for i, line := range lines {
		t := strings.TrimSpace(line)
		if strings.HasPrefix(t, "- [ ] ") || strings.HasPrefix(t, "- [x] ") {
			seen++
			if seen == idx {
				lines[i] = strings.Replace(line, "- [ ] ", "- [x] ", 1)
				break
			}
		}
	}
	return os.WriteFile(l.path(reqID), []byte(strings.Join(lines, "\n")), 0o644)
}

// Render returns the ledger markdown for injection into the model context, or ""
// when there is no ledger for reqID.
func (l Ledger) Render(reqID string) string {
	b, err := os.ReadFile(l.path(reqID))
	if err != nil {
		return ""
	}
	return string(b)
}
