package framing

import (
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/rtmx"
)

func reqs() []*rtmx.Requirement {
	return []*rtmx.Requirement{
		{ID: "REQ-A-1", Status: rtmx.StatusClosed, SpecFile: "docs/requirements/a.md"},
		{ID: "REQ-A-2", Status: rtmx.StatusOpen, Notes: "spec: docs/requirements/a.md"},
		{ID: "REQ-A-3", Status: rtmx.StatusBlocked, Notes: "spec: docs/requirements/a.md"},
		{ID: "REQ-A-4", Status: rtmx.StatusProposed, SpecFile: "docs/requirements/a.md"},
		{ID: "REQ-OLD-1", Status: rtmx.StatusOpen}, // no framing artifact → unframed
	}
}

// TestClassifyDiscoveryBacklog → FRAME-002: parked requirements are surfaced as
// the reframe backlog, and the delivery lanes are classified.
func TestClassifyDiscoveryBacklog(t *testing.T) {
	c := Classify(reqs())
	if len(c.Parked) != 1 || c.Parked[0] != "REQ-A-3" {
		t.Errorf("reframe backlog (parked) wrong: %v", c.Parked)
	}
	if len(c.Delivered) != 1 || c.Delivered[0] != "REQ-A-1" {
		t.Errorf("delivered lane wrong: %v", c.Delivered)
	}
	if len(c.Proposed) != 1 || c.Proposed[0] != "REQ-A-4" {
		t.Errorf("proposed lane wrong: %v", c.Proposed)
	}
	if len(c.InFlight) != 2 {
		t.Errorf("in-flight lane wrong: %v", c.InFlight)
	}
	if got := ReframeBacklog(reqs()); len(got) != 1 || got[0] != "REQ-A-3" {
		t.Errorf("ReframeBacklog wrong: %v", got)
	}
}

// TestFramingHygiene → FRAME-003: requirements with no framing artifact are
// flagged unframed; framed ones (spec file or "spec:" note) are not.
func TestFramingHygiene(t *testing.T) {
	c := Classify(reqs())
	if len(c.Unframed) != 1 || c.Unframed[0] != "REQ-OLD-1" {
		t.Errorf("unframed detection wrong: %v", c.Unframed)
	}
	if IsFramed(&rtmx.Requirement{}) {
		t.Error("a requirement with no spec/notes must be unframed")
	}
	if !IsFramed(&rtmx.Requirement{SpecFile: "x.md"}) {
		t.Error("a requirement with a spec file must be framed")
	}
	if !IsFramed(&rtmx.Requirement{Notes: "spec: y.md"}) {
		t.Error("a requirement with a spec: note must be framed")
	}
}
