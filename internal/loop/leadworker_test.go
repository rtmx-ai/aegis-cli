package loop

import "testing"

// TestLeadWorkerSplit → REQ-THINK-007: phases map to roles (plan->Lead, exec->Worker),
// an enabled split routes lead vs worker models, and the gate collapses to a single
// model when disabled.
func TestLeadWorkerSplit(t *testing.T) {
	if RoleFor(true) != Lead {
		t.Error("planning phase must route to Lead")
	}
	if RoleFor(false) != Worker {
		t.Error("execution phase must route to Worker")
	}

	// Enabled split routes each role to its quant.
	p := LeadWorkerPolicy{Enabled: true, LeadModel: "q6", WorkerModel: "q4"}
	if p.Route(Lead) != "q6" {
		t.Errorf("lead role -> lead model, got %q", p.Route(Lead))
	}
	if p.Route(Worker) != "q4" {
		t.Errorf("worker role -> worker model, got %q", p.Route(Worker))
	}

	// Gated off: everything routes to the single lead model (no split).
	off := LeadWorkerPolicy{Enabled: false, LeadModel: "q6", WorkerModel: "q4"}
	if off.Route(Worker) != "q6" {
		t.Errorf("disabled split must route worker to the single model, got %q", off.Route(Worker))
	}
}
