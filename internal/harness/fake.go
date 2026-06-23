package harness

import (
	"context"
	"fmt"

	"github.com/rtmx-ai/aegis-cli/internal/rtmx"
)

// Fake is an in-memory Adapter for tests. It returns a canned Diff and can
// simulate malformed-tool-call retries and outright failures.
type Fake struct {
	// AdapterName is reported by Name.
	AdapterName string
	// DriveErr, if set, is returned by Drive.
	DriveErr error
	// Unhealthy, when true, makes Health return an error.
	Unhealthy bool
	// MalformedThenOK simulates one malformed tool call that the adapter
	// detects and retries: the returned Diff reflects the retry, with one
	// extra (invalid) tool call counted but a successful patch.
	MalformedThenOK bool
	// Calls counts Drive invocations.
	Calls int
}

// NewFake returns a Fake with a default name.
func NewFake() *Fake {
	return &Fake{AdapterName: "fake"}
}

// Name reports the adapter identity.
func (f *Fake) Name() string {
	if f.AdapterName == "" {
		return "fake"
	}
	return f.AdapterName
}

// Drive returns a canned Diff, modeling a retried malformed tool call when
// MalformedThenOK is set.
func (f *Fake) Drive(ctx context.Context, req *rtmx.Requirement) (Diff, error) {
	f.Calls++
	if f.DriveErr != nil {
		return Diff{RequirementID: req.ID}, f.DriveErr
	}
	d := Diff{
		RequirementID:  req.ID,
		Patch:          fmt.Sprintf("--- a/%s\n+++ b/%s\n", req.ID, req.ID),
		Turns:          1,
		ToolCalls:      1,
		ValidToolCalls: 1,
		Tokens:         100,
	}
	if f.MalformedThenOK {
		// One malformed call detected and retried: total calls 2, valid 1.
		d.ToolCalls = 2
		d.ValidToolCalls = 1
		d.Turns = 2
	}
	return d, nil
}

// Health returns an error only when Unhealthy is set.
func (f *Fake) Health(ctx context.Context) error {
	if f.Unhealthy {
		return fmt.Errorf("harness: unhealthy")
	}
	return nil
}
