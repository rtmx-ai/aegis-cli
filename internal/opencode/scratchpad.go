package opencode

import (
	"os"
	"strings"
)

// Scratchpad is an append-only, durable notes file for a long task (MEM-001):
// discoveries, decisions, and running state the agent records as it works, distinct
// from human-authored intent (CLAUDE.md / AGENTS.md), which it never touches. It is
// re-injected into context and survives compaction + resume because it lives on
// disk, not the model's window — free-form, unlike the LONGRUN-003 checklist ledger
// and the MEM-005 structured fact store.
type Scratchpad struct {
	Path string
}

// Append records a note. Append-only: notes accumulate in order; nothing is
// overwritten (a resume keeps the full running history).
func (s Scratchpad) Append(note string) error {
	if err := GuardIntentWrite(s.Path); err != nil { // MEM-004: never write machine memory to intent files
		return err
	}
	note = strings.TrimSpace(note)
	if note == "" {
		return nil
	}
	f, err := os.OpenFile(s.Path, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o644)
	if err != nil {
		return err
	}
	defer f.Close()
	_, err = f.WriteString("- " + note + "\n")
	return err
}

// Render returns the scratchpad notes for injection into the model context ("" when
// empty).
func (s Scratchpad) Render() string {
	b, err := os.ReadFile(s.Path)
	if err != nil || len(b) == 0 {
		return ""
	}
	return "# Task scratchpad (running notes; machine-written)\n\n" + string(b)
}
