// Package rtmx is the client to the rtmx requirements engine.
//
// In production it speaks MCP over stdio with a CLI fallback; the loop depends
// only on the Client interface defined here, so a stub/fake can stand in for
// tests. Nothing in this package makes a network call.
package rtmx

import "context"

// Status is a requirement's lifecycle state in the rtmx database.
type Status string

// Requirement lifecycle states. "proposed" requirements are not claimable by
// the loop until a human promotes them (see internal/propose).
const (
	StatusOpen     Status = "open"
	StatusClaimed  Status = "claimed"
	StatusClosed   Status = "closed"
	StatusBlocked  Status = "blocked"
	StatusProposed Status = "proposed"
)

// Requirement is a single rtmx requirement record.
type Requirement struct {
	// ID is the unique requirement identifier (e.g. "LOOP-001").
	ID string `json:"id"`
	// Prefix is the category prefix (e.g. "LOOP").
	Prefix string `json:"prefix"`
	// Title is the human-readable summary.
	Title string `json:"title"`
	// Status is the lifecycle state.
	Status Status `json:"status"`
	// Tests are the acceptance-test references that close the requirement.
	Tests []string `json:"tests"`
	// Deps are the IDs of requirements this one depends on.
	Deps []string `json:"deps"`
	// SpecFile links the framing artifact (the requirement_file column), if any.
	SpecFile string `json:"spec_file,omitempty"`
	// Notes is the free-form notes column (may carry a "spec:" reference).
	Notes string `json:"notes,omitempty"`
}

// Client is the contract the loop uses to drive rtmx.
type Client interface {
	// Next returns the next available (claimable) requirement, or nil when the
	// backlog is empty. Proposed requirements are never returned.
	Next(ctx context.Context) (*Requirement, error)
	// Claim atomically claims a requirement; it fails if already claimed.
	Claim(ctx context.Context, id string) error
	// Release returns a claimed requirement to the backlog.
	Release(ctx context.Context, id string) error
	// Verify runs the requirement's acceptance tests and returns the result plus
	// the test output (the failure text, fed back into the next drive; LONGRUN-001).
	Verify(ctx context.Context, id string) (ok bool, output string, err error)
	// WriteStatus writes a status back to the rtmx database.
	WriteStatus(ctx context.Context, id string, status Status) error
	// Health reports whether the engine is reachable and the database is sound.
	Health(ctx context.Context) error
}
