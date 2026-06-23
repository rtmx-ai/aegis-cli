// Package metrics collects per-run orchestrator metrics and emits a report.
//
// The north-star metric is ACR (Autonomous Completion Rate). The dashboard
// also tracks TCVR, FPVR, MTC, WCR, TCR and ESC, plus a per-stage timing
// breakdown (prefill/decode/verify/harness-overhead) that doubles as the
// profiler. Everything here is in-process; nothing phones home.
package metrics

import (
	"encoding/json"
	"time"
)

// Stages holds per-stage timing for one requirement attempt.
type Stages struct {
	// Prefill is model prompt-processing time.
	Prefill time.Duration `json:"prefill"`
	// Decode is model generation time.
	Decode time.Duration `json:"decode"`
	// Verify is requirement-verification time.
	Verify time.Duration `json:"verify"`
	// HarnessOverhead is time spent in the harness outside model calls.
	HarnessOverhead time.Duration `json:"harness_overhead"`
}

// Attempt records the outcome of a single requirement attempt.
type Attempt struct {
	// RequirementID is the requirement attempted.
	RequirementID string `json:"requirement_id"`
	// Closed is true if verify passed and the requirement was closed.
	Closed bool `json:"closed"`
	// Escalated is true if the requirement was handed to a human / parked.
	Escalated bool `json:"escalated"`
	// FirstPass is true if it closed without a retry.
	FirstPass bool `json:"first_pass"`
	// Turns is the number of agent round-trips.
	Turns int `json:"turns"`
	// ToolCalls is the total tool calls emitted.
	ToolCalls int `json:"tool_calls"`
	// ValidToolCalls is the count of well-formed tool calls.
	ValidToolCalls int `json:"valid_tool_calls"`
	// Tokens is total tokens (including reasoning) consumed.
	Tokens int `json:"tokens"`
	// WallClock is end-to-end attempt latency.
	WallClock time.Duration `json:"wall_clock"`
	// Stages is the per-stage timing breakdown.
	Stages Stages `json:"stages"`
}

// Report is a JSON-serializable per-run metrics report.
type Report struct {
	// Attempted is the number of requirements attempted.
	Attempted int `json:"attempted"`
	// Closed is the number closed by verify with no human step.
	Closed int `json:"closed"`
	// Escalated is the number handed to a human.
	Escalated int `json:"escalated"`
	// ACR is the Autonomous Completion Rate (north star).
	ACR float64 `json:"acr"`
	// TCVR is the Tool-Call Validity Rate.
	TCVR float64 `json:"tcvr"`
	// FPVR is the First-Pass Verify Rate.
	FPVR float64 `json:"fpvr"`
	// MTC is the Mean Turns-to-Close.
	MTC float64 `json:"mtc"`
	// WCR is the mean Wall-Clock per Requirement.
	WCR time.Duration `json:"wcr"`
	// TCR is the mean Token Cost per Requirement.
	TCR float64 `json:"tcr"`
	// ESC is the Escalation Rate.
	ESC float64 `json:"esc"`
	// Stages is the aggregate per-stage timing across attempts.
	Stages Stages `json:"stages"`
}

// Collector accumulates attempts and produces a Report.
type Collector struct {
	attempts []Attempt
}

// NewCollector returns an empty Collector.
func NewCollector() *Collector {
	return &Collector{}
}

// Record adds one attempt to the collection.
func (c *Collector) Record(a Attempt) {
	c.attempts = append(c.attempts, a)
}

// Report computes aggregate metrics over all recorded attempts.
func (c *Collector) Report() Report {
	var r Report
	r.Attempted = len(c.attempts)
	if r.Attempted == 0 {
		return r
	}

	var (
		firstPass           int
		closedTurns         int
		totalTurns          int
		totalToolCalls      int
		totalValidToolCalls int
		totalTokens         int
		totalWallClock      time.Duration
	)
	for _, a := range c.attempts {
		if a.Closed {
			r.Closed++
			closedTurns += a.Turns
			if a.FirstPass {
				firstPass++
			}
		}
		if a.Escalated {
			r.Escalated++
		}
		totalTurns += a.Turns
		totalToolCalls += a.ToolCalls
		totalValidToolCalls += a.ValidToolCalls
		totalTokens += a.Tokens
		totalWallClock += a.WallClock
		r.Stages.Prefill += a.Stages.Prefill
		r.Stages.Decode += a.Stages.Decode
		r.Stages.Verify += a.Stages.Verify
		r.Stages.HarnessOverhead += a.Stages.HarnessOverhead
	}

	attempted := float64(r.Attempted)
	r.ACR = float64(r.Closed) / attempted
	r.ESC = float64(r.Escalated) / attempted
	r.WCR = totalWallClock / time.Duration(r.Attempted)
	r.TCR = float64(totalTokens) / attempted
	if totalToolCalls > 0 {
		r.TCVR = float64(totalValidToolCalls) / float64(totalToolCalls)
	}
	if r.Closed > 0 {
		r.FPVR = float64(firstPass) / float64(r.Closed)
		r.MTC = float64(closedTurns) / float64(r.Closed)
	}
	_ = totalTurns
	return r
}

// JSON serializes the report to indented JSON.
func (r Report) JSON() ([]byte, error) {
	return json.MarshalIndent(r, "", "  ")
}
