package harness

import (
	"context"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/rtmx"
)

// TestToolCallMalformedRetriedNotCrashed asserts that a malformed tool call is
// detected and retried inside the adapter, surfacing as an extra (invalid) tool
// call in the Diff rather than an error.
func TestToolCallMalformedRetriedNotCrashed(t *testing.T) {
	f := NewFake()
	f.MalformedThenOK = true
	d, err := f.Drive(context.Background(), &rtmx.Requirement{ID: "A-001"}, "")
	if err != nil {
		t.Fatalf("malformed tool call must not crash the drive: %v", err)
	}
	if d.ToolCalls <= d.ValidToolCalls {
		t.Fatalf("expected a detected-invalid call: total=%d valid=%d", d.ToolCalls, d.ValidToolCalls)
	}
	if d.Patch == "" {
		t.Fatal("retry should still produce a patch")
	}
}
