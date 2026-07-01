package loop

// Role is a phase in the two-quant lead/worker split (THINK-007).
type Role int

const (
	// Lead is the higher-quant model: planning / hard reasoning.
	Lead Role = iota
	// Worker is the faster low-quant model: executing edits.
	Worker
)

// LeadWorkerPolicy routes a task phase to the lead (higher-quant) or worker
// (lower-quant) model in the two-quant split (THINK-007): the lead plans, the
// worker executes. Gated + experimental — disabled unless two endpoints are
// configured, because a small host rarely has the RAM (and memory bandwidth) for
// two resident quants, and one bus can't feed both at once.
type LeadWorkerPolicy struct {
	Enabled     bool
	LeadModel   string
	WorkerModel string
}

// RoleFor maps a phase to a role: planning -> Lead, execution -> Worker (THINK-007).
func RoleFor(planning bool) Role {
	if planning {
		return Lead
	}
	return Worker
}

// Route returns the model endpoint for a role, honoring the gate: when disabled,
// every role routes to the single lead model (no split).
func (p LeadWorkerPolicy) Route(r Role) string {
	if p.Enabled && r == Worker && p.WorkerModel != "" {
		return p.WorkerModel
	}
	return p.LeadModel
}
