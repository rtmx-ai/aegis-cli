package rtmx

import (
	"context"
	"fmt"
	"sync"
)

// Fake is an in-memory Client for tests. It models atomic claim/release,
// verify outcomes, and status writeback without any external process.
type Fake struct {
	mu sync.Mutex
	// Reqs is the backlog, in priority order.
	Reqs []*Requirement
	// VerifyResult maps requirement ID to the result Verify will return.
	// A missing entry verifies false.
	VerifyResult map[string]bool
	// VerifyOutput maps requirement ID to the output Verify returns (the test
	// failure text fed back into the next drive; LONGRUN-001).
	VerifyOutput map[string]string
	// VerifyErr, if set for an ID, makes Verify return that error.
	VerifyErr map[string]error
	// Unhealthy, when true, makes Health return an error.
	Unhealthy bool
	// claimed tracks currently-claimed IDs to enforce no double-claim.
	claimed map[string]bool
}

// NewFake returns a Fake seeded with the given requirements.
func NewFake(reqs ...*Requirement) *Fake {
	return &Fake{
		Reqs:         reqs,
		VerifyResult: map[string]bool{},
		VerifyOutput: map[string]string{},
		VerifyErr:    map[string]error{},
		claimed:      map[string]bool{},
	}
}

// Next returns the first claimable, unclaimed, non-proposed requirement.
func (f *Fake) Next(ctx context.Context) (*Requirement, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	for _, r := range f.Reqs {
		if r.Status == StatusClosed || r.Status == StatusProposed || r.Status == StatusBlocked {
			continue
		}
		if f.claimed[r.ID] {
			continue
		}
		return r, nil
	}
	return nil, nil
}

// Claim atomically claims id, failing on a double-claim.
func (f *Fake) Claim(ctx context.Context, id string) error {
	f.mu.Lock()
	defer f.mu.Unlock()
	if f.claimed[id] {
		return fmt.Errorf("rtmx: %s already claimed", id)
	}
	f.claimed[id] = true
	return nil
}

// Release returns id to the backlog.
func (f *Fake) Release(ctx context.Context, id string) error {
	f.mu.Lock()
	defer f.mu.Unlock()
	delete(f.claimed, id)
	return nil
}

// Verify returns the configured outcome for id.
func (f *Fake) Verify(ctx context.Context, id string) (bool, string, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	if err := f.VerifyErr[id]; err != nil {
		return false, "", err
	}
	return f.VerifyResult[id], f.VerifyOutput[id], nil
}

// WriteStatus updates the in-memory requirement status.
func (f *Fake) WriteStatus(ctx context.Context, id string, status Status) error {
	f.mu.Lock()
	defer f.mu.Unlock()
	for _, r := range f.Reqs {
		if r.ID == id {
			r.Status = status
			return nil
		}
	}
	return fmt.Errorf("rtmx: unknown requirement %s", id)
}

// Health returns an error only when Unhealthy is set.
func (f *Fake) Health(ctx context.Context) error {
	f.mu.Lock()
	defer f.mu.Unlock()
	if f.Unhealthy {
		return fmt.Errorf("rtmx: unhealthy")
	}
	return nil
}
