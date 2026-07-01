package harness

import (
	"context"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/rtmx"
)

func TestDriveProducesDiff(t *testing.T) {
	f := NewFake()
	req := &rtmx.Requirement{ID: "A-001"}
	d, err := f.Drive(context.Background(), req, "")
	if err != nil {
		t.Fatalf("drive: %v", err)
	}
	if d.RequirementID != "A-001" {
		t.Errorf("diff req id = %q, want A-001", d.RequirementID)
	}
	if d.Patch == "" {
		t.Error("drive should produce a non-empty patch")
	}
}

// Fake satisfies the Adapter interface.
var _ Adapter = (*Fake)(nil)
