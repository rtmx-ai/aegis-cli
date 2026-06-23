// Package framing turns delivery evidence (the rtmx backlog) into the inputs of
// the discovery/framing loop: it classifies requirements into delivery lanes and
// surfaces the reframe backlog (parked work) and framing-hygiene gaps (untraced
// work). It is assistive only — it surfaces; a human frames and approves. See
// docs/requirements/discovery-framing.md (FRAME-002/003) and skills/discovery.
package framing

import (
	"strings"

	"github.com/rtmx-ai/aegis-cli/internal/rtmx"
)

// Classification is the five-way view of the backlog.
type Classification struct {
	// Delivered are requirements closed by verify — terminated in functional value.
	Delivered []string
	// InFlight are open, claimable requirements.
	InFlight []string
	// Parked are blocked requirements — the discovery (reframe) backlog.
	Parked []string
	// Proposed are machine-proposed requirements awaiting human framing approval.
	Proposed []string
	// Unframed are requirements with no framing artifact (a hygiene gap). It is a
	// cross-cutting view (a requirement can be both, e.g. in-flight and unframed).
	Unframed []string
}

// IsFramed reports whether a requirement traces to a framing artifact: a
// requirement_file link or a "spec:" reference in its notes.
func IsFramed(r *rtmx.Requirement) bool {
	return strings.TrimSpace(r.SpecFile) != "" || strings.Contains(r.Notes, "spec:")
}

// Classify sorts requirements into delivery lanes and the cross-cutting unframed
// list, preserving input order within each lane.
func Classify(reqs []*rtmx.Requirement) Classification {
	var c Classification
	for _, r := range reqs {
		switch r.Status {
		case rtmx.StatusClosed:
			c.Delivered = append(c.Delivered, r.ID)
		case rtmx.StatusBlocked:
			c.Parked = append(c.Parked, r.ID)
		case rtmx.StatusProposed:
			c.Proposed = append(c.Proposed, r.ID)
		default:
			c.InFlight = append(c.InFlight, r.ID)
		}
		if !IsFramed(r) {
			c.Unframed = append(c.Unframed, r.ID)
		}
	}
	return c
}

// ReframeBacklog returns the parked requirements — the work the loop could not
// close, which is the human's discovery/reframe input (not a delivery failure).
func ReframeBacklog(reqs []*rtmx.Requirement) []string {
	return Classify(reqs).Parked
}
