package loop

import (
	"strings"

	"github.com/rtmx-ai/aegis-cli/internal/rtmx"
)

// Handoff is the grounded continuity record carried across a context compaction
// (LONGRUN-006): the requirement, the files touched, key decisions, and the next
// concrete step — so a lossy summary doesn't drop what the run needs to continue.
type Handoff struct {
	RequirementID string
	Title         string
	FilesTouched  []string
	Decisions     []string
	NextStep      string
}

// BuildHandoff assembles a grounded handoff from the requirement and the drive
// trace (files touched are derived from edit/write tool calls), plus recorded
// decisions and the next concrete step (LONGRUN-006).
func BuildHandoff(req *rtmx.Requirement, trace []Step, decisions []string, next string) Handoff {
	h := Handoff{Decisions: decisions, NextStep: next}
	if req != nil {
		h.RequirementID = req.ID
		h.Title = req.Title
	}
	h.FilesTouched = filesFromTrace(trace)
	return h
}

func filesFromTrace(trace []Step) []string {
	edits := map[string]bool{"edit": true, "write": true, "create": true, "patch": true, "str_replace": true}
	seen := map[string]bool{}
	var out []string
	for _, s := range trace {
		if !edits[strings.ToLower(s.Tool)] {
			continue
		}
		if f := strings.TrimSpace(s.Args); f != "" && !seen[f] {
			seen[f] = true
			out = append(out, f)
		}
	}
	return out
}

// Render returns the handoff as a compact markdown block that seeds the post-
// compaction context, preserving continuity (LONGRUN-006).
func (h Handoff) Render() string {
	var b strings.Builder
	b.WriteString("# Handoff (continuity across compaction)\n\n")
	b.WriteString("Requirement: " + h.RequirementID)
	if h.Title != "" {
		b.WriteString(" — " + h.Title)
	}
	b.WriteString("\n")
	if len(h.FilesTouched) > 0 {
		b.WriteString("Files touched: " + strings.Join(h.FilesTouched, ", ") + "\n")
	}
	for _, d := range h.Decisions {
		b.WriteString("Decision: " + d + "\n")
	}
	if h.NextStep != "" {
		b.WriteString("Next step: " + h.NextStep + "\n")
	}
	return b.String()
}
