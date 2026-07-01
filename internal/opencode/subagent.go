package opencode

import "fmt"

// SubagentPolicy governs sub-agent delegation (LONGRUN-005): delegate a bounded
// sub-task to a fresh-context sub-agent one at a time — NEVER in parallel. A small
// local model sharing one memory bus can't afford concurrent inference; two
// bandwidth-heavy generations at once each get slower (the resource model in
// CLAUDE.md — separate in time, don't overlap on the bus).
type SubagentPolicy struct {
	// MaxConcurrent is always 1 for aegis; a value > 1 is rejected.
	MaxConcurrent int
	// MaxSubtasks bounds how many sub-tasks a single task may delegate.
	MaxSubtasks int
}

// DefaultSubagentPolicy returns aegis's sequential delegation policy.
func DefaultSubagentPolicy() SubagentPolicy {
	return SubagentPolicy{MaxConcurrent: 1, MaxSubtasks: 4}
}

// Validate rejects a non-sequential policy (LONGRUN-005).
func (p SubagentPolicy) Validate() error {
	if p.MaxConcurrent != 1 {
		return fmt.Errorf("opencode: sub-agent delegation must be sequential (MaxConcurrent=1), got %d (LONGRUN-005)", p.MaxConcurrent)
	}
	if p.MaxSubtasks <= 0 {
		return fmt.Errorf("opencode: sub-task bound must be positive, got %d (LONGRUN-005)", p.MaxSubtasks)
	}
	return nil
}
