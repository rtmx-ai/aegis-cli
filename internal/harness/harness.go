// Package harness defines the coding-agent adapter interface and stub impls.
//
// aegis-cli is not a harness: it delegates all tool-calling, file editing, and
// sandboxing to an external harness (opencode or goose) behind the Adapter
// interface. Malformed tool calls are detected and retried by the adapter
// rather than crashing the loop.
package harness

import (
	"context"

	"github.com/rtmx-ai/aegis-cli/internal/rtmx"
)

// Diff is the result of driving a requirement: the proposed change plus the
// signals the metrics layer needs.
type Diff struct {
	// RequirementID is the requirement that was driven.
	RequirementID string
	// Patch is the unified diff the harness produced (may be empty on failure).
	Patch string
	// Turns is the number of agent round-trips taken.
	Turns int
	// ToolCalls is the total tool calls emitted.
	ToolCalls int
	// ValidToolCalls is the count of well-formed tool calls.
	ValidToolCalls int
	// Tokens is total tokens consumed.
	Tokens int
}

// Adapter drives a single requirement headless to a Diff.
type Adapter interface {
	// Name reports the adapter's identity (e.g. "opencode").
	Name() string
	// Drive runs the harness on req and returns the produced Diff. A malformed
	// tool call must be detected and retried internally, not surfaced as a crash.
	Drive(ctx context.Context, req *rtmx.Requirement) (Diff, error)
	// Health reports whether the harness is launchable and configured offline.
	Health(ctx context.Context) error
}
