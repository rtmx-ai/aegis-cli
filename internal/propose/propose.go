// Package propose implements human-gated requirement decomposition.
//
// The machine proposes atomic children of a coarse requirement; it never
// auto-approves them. Proposed children land in a non-claimable "proposed"
// state, inherit the parent's acceptance tests, are bounded by a depth limit
// and a child cap, and are each tagged machine-authored in the audit trail.
// See skills/decomposition/SKILL.md.
package propose

import (
	"context"
	"fmt"

	"github.com/rtmx-ai/aegis-cli/internal/audit"
	"github.com/rtmx-ai/aegis-cli/internal/rtmx"
)

// Bounds limits a decomposition so a split cannot run away.
type Bounds struct {
	// MaxDepth is the maximum decomposition depth (1 = direct children only).
	MaxDepth int
	// MaxChildren is the maximum number of children per parent.
	MaxChildren int
}

// DefaultBounds returns conservative decomposition bounds.
func DefaultBounds() Bounds {
	return Bounds{MaxDepth: 1, MaxChildren: 8}
}

// Proposal is the result of decomposing a parent requirement.
type Proposal struct {
	// Parent is the requirement that was decomposed.
	Parent string
	// Children are the proposed atomic children, all in StatusProposed.
	Children []rtmx.Requirement
}

// Proposer turns coarse requirements into bounded, human-gated proposals.
type Proposer struct {
	// Bounds limits the split.
	Bounds Bounds
	// Audit records machine-authored provenance for every child.
	Audit *audit.Log
}

// New returns a Proposer with the given bounds and audit log.
func New(bounds Bounds, log *audit.Log) *Proposer {
	return &Proposer{Bounds: bounds, Audit: log}
}

// Propose decomposes parent into atomic child requirements described by titles.
// Each child inherits the parent's tests, is set to StatusProposed (not
// claimable), and is recorded machine-authored in the audit trail. Exceeding
// the child cap is a hard stop that asks for a human decision.
func (p *Proposer) Propose(ctx context.Context, parent rtmx.Requirement, childTitles []string) (*Proposal, error) {
	if p.Bounds.MaxDepth < 1 {
		return nil, fmt.Errorf("propose: max depth must be >= 1")
	}
	if len(childTitles) == 0 {
		return nil, fmt.Errorf("propose: no children to propose for %s", parent.ID)
	}
	if len(childTitles) > p.Bounds.MaxChildren {
		return nil, fmt.Errorf("propose: %d children exceeds cap %d for %s (human decision required)",
			len(childTitles), p.Bounds.MaxChildren, parent.ID)
	}

	prop := &Proposal{Parent: parent.ID}
	for i, title := range childTitles {
		child := rtmx.Requirement{
			ID:     fmt.Sprintf("%s-P%02d", parent.ID, i+1),
			Prefix: parent.Prefix,
			Title:  title,
			Status: rtmx.StatusProposed,
			Tests:  append([]string(nil), parent.Tests...), // inherit parent tests
			Deps:   []string{parent.ID},
		}
		prop.Children = append(prop.Children, child)
		if p.Audit != nil {
			if err := p.Audit.Record(audit.Entry{
				Action:          audit.ActionPropose,
				RequirementID:   child.ID,
				Result:          string(rtmx.StatusProposed),
				MachineAuthored: true,
				Detail:          fmt.Sprintf("proposed child of %s", parent.ID),
			}); err != nil {
				return nil, fmt.Errorf("propose: audit child %s: %w", child.ID, err)
			}
		}
	}
	return prop, nil
}
