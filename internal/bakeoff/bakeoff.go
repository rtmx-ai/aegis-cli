// Package bakeoff is the model bake-off measurement rig (BENCH-010): it runs a fixed suite of agentic
// coding tasks across candidate models on the REAL serve→opencode→verify path and scores each on the
// axes that decide "can this model write code on this host" —
//
//   - EditRate : fraction of tasks where the model actually changed a file on disk (the agency headline;
//     a hollow, no-edit answer scores 0 here even if it "talked about" the change)
//   - ACR      : fraction closed by verify with no human step (correctness — did the edit pass the test)
//   - TokPerSec: measured decode throughput (is it usable on this host — the dense-vs-MoE axis)
//   - the internal/metrics dashboard (TCVR/FPVR/MTC/WCR/TCR), reused, not re-derived
//
// The point of the rig is that "shallow/dry" and "too slow" become numbers we compare, not impressions.
// The driving of each cell is injected (Driver), so the aggregation/comparison/report core is unit-tested
// off-box; the live driver runs on the target (a 24GB M5 can serve gemma fast but a 24B dense model slow).
package bakeoff

import (
	"fmt"
	"sort"
	"strings"
	"time"

	"github.com/rtmx-ai/aegis-cli/internal/metrics"
)

// Task is one agentic coding task: seed files that fail, a prompt, and an objective verify command whose
// exit 0 means the task is closed. Kept in Go so scoring needs no extra runtime (matches serve-bakeoff).
type Task struct {
	Name   string            `json:"name"`
	Prompt string            `json:"prompt"`
	Files  map[string]string `json:"files"`
	Verify []string          `json:"verify"`
}

// Outcome is one (model, task) cell result.
type Outcome struct {
	Task           string `json:"task"`
	FilesEdited    int    `json:"files_edited"` // files changed on disk (git) — the agency ground truth
	Closed         bool   `json:"closed"`       // verify passed
	FirstPass      bool   `json:"first_pass"`
	Escalated      bool   `json:"escalated"`
	ToolCalls      int    `json:"tool_calls"`
	ValidToolCalls int    `json:"valid_tool_calls"`
	Turns          int    `json:"turns"`
	Tokens         int    `json:"tokens"`     // total (input+output) — cost per requirement (TCR)
	OutTokens      int    `json:"out_tokens"` // output/decode tokens — for honest tok/s (not prefill)
	WallMs         int64  `json:"wall_ms"`
	Error          string `json:"error,omitempty"`
}

// CandidateReport aggregates a model's outcomes over the suite.
type CandidateReport struct {
	Model string `json:"model"`
	// ServedModel is the model the endpoint ACTUALLY served (from GET /v1/models) while these outcomes
	// were collected — recorded so a comparison can prove each candidate ran on its own model. Two
	// candidates reporting the SAME served model means the endpoint was never swapped (the results are
	// the same model twice) and Compare invalidates the head-to-head.
	ServedModel string         `json:"served_model"`
	Attempts    int            `json:"attempts"`
	Edited      int            `json:"edited"`      // cells where >=1 file changed
	EditRate    float64        `json:"edit_rate"`   // did it write code? (headline)
	TokPerSec   float64        `json:"tok_per_sec"` // OUTPUT tokens / wall — honest decode-ish throughput
	Report      metrics.Report `json:"report"`      // ACR/TCVR/FPVR/MTC/WCR/TCR — reused
	Outcomes    []Outcome      `json:"outcomes"`
}

// Aggregate builds a CandidateReport from a model's outcomes, feeding internal/metrics for the dashboard
// so the bake-off and CI report the SAME metric definitions (no divergent math). servedModel is what the
// endpoint reported serving during the run (for the same-model guard); "" if it could not be probed.
func Aggregate(model, servedModel string, outs []Outcome) CandidateReport {
	col := metrics.NewCollector()
	edited := 0
	var wallMs, outTokens int64
	for _, o := range outs {
		col.Record(metrics.Attempt{
			RequirementID:  o.Task,
			Closed:         o.Closed,
			Escalated:      o.Escalated,
			FirstPass:      o.FirstPass,
			Turns:          o.Turns,
			ToolCalls:      o.ToolCalls,
			ValidToolCalls: o.ValidToolCalls,
			Tokens:         o.Tokens,
			WallClock:      time.Duration(o.WallMs) * time.Millisecond,
		})
		if o.FilesEdited > 0 {
			edited++
		}
		wallMs += o.WallMs
		outTokens += int64(o.OutTokens)
	}
	tps := 0.0
	if wallMs > 0 {
		// OUTPUT tokens / wall — not (input+output)/wall, which is prefill-dominated and meaningless.
		tps = float64(outTokens) / (float64(wallMs) / 1000.0)
	}
	er := 0.0
	if n := len(outs); n > 0 {
		er = float64(edited) / float64(n)
	}
	return CandidateReport{
		Model: model, ServedModel: servedModel, Attempts: len(outs), Edited: edited,
		EditRate: er, TokPerSec: tps, Report: col.Report(), Outcomes: outs,
	}
}

// Comparison is the head-to-head result over one suite on one host.
type Comparison struct {
	Suite      string            `json:"suite"`
	Host       string            `json:"host"`
	Candidates []CandidateReport `json:"candidates"`
	Winner     string            `json:"winner"`
	Basis      string            `json:"basis"`
}

// Compare ranks candidates: AGENCY first (EditRate, then ACR), then THROUGHPUT (tok/s), then latency
// (WCR). The ordering encodes the project's judgment — a fast model that writes no code loses to a
// slower one that actually edits and closes; among models that write and close, the faster wins. If no
// candidate ever wrote a file, there is no winner (the suite/host beat the whole field) — say so rather
// than crown the fastest do-nothing.
func Compare(suite, host string, reports []CandidateReport) Comparison {
	ranked := append([]CandidateReport(nil), reports...)
	sort.SliceStable(ranked, func(i, j int) bool {
		a, b := ranked[i], ranked[j]
		if a.EditRate != b.EditRate {
			return a.EditRate > b.EditRate
		}
		if a.Report.ACR != b.Report.ACR {
			return a.Report.ACR > b.Report.ACR
		}
		if a.TokPerSec != b.TokPerSec {
			return a.TokPerSec > b.TokPerSec
		}
		return a.Report.WCR < b.Report.WCR
	})
	c := Comparison{Suite: suite, Host: host, Candidates: ranked}
	// Same-model guard: if two candidates report the SAME served model, the endpoint was never swapped —
	// the run measured one model twice, so any "winner" is noise. Refuse to rank (this is the exact trap
	// the first bake-off fell into: near-identical token counts because both cells hit one served model).
	if dup := duplicateServedModel(ranked); dup != "" {
		c.Basis = "INVALID: multiple candidates were served by the same model (" + dup + ") — the endpoint was not swapped between candidates, so this is one model measured twice. Serve each candidate on its own endpoint (or use --serve) and re-run."
		return c
	}
	if len(ranked) == 0 || ranked[0].EditRate == 0 {
		c.Basis = "no candidate edited a file on any task — the whole field failed the agency bar (check serving/template, not the models)"
		return c
	}
	c.Winner = ranked[0].Model
	c.Basis = "agency first (edit-rate, then ACR), then throughput (tok/s), then wall-clock — a fast model that writes no code loses to a slower one that edits and closes"
	return c
}

// duplicateServedModel returns a served-model id shared by >1 candidate (the same-endpoint trap), or "".
func duplicateServedModel(rs []CandidateReport) string {
	seen := map[string]int{}
	for _, r := range rs {
		if r.ServedModel != "" {
			seen[r.ServedModel]++
		}
	}
	for m, n := range seen {
		if n > 1 {
			return m
		}
	}
	return ""
}

// Table renders the head-to-head as a fixed-width table, agency columns first (the question that matters).
func (c Comparison) Table() string {
	var b strings.Builder
	fmt.Fprintf(&b, "bake-off: %s on %s\n", c.Suite, c.Host)
	fmt.Fprintf(&b, "%-22s %8s %7s %7s %9s %9s %-18s\n",
		"model", "edited", "ACR", "TCVR", "out-tok/s", "wall/req", "served-as")
	for _, r := range c.Candidates {
		served := r.ServedModel
		if served == "" {
			served = "?"
		}
		fmt.Fprintf(&b, "%-22s %5d/%-2d %6.0f%% %6.0f%% %9.1f %8.0fs %-18s\n",
			trunc(r.Model, 22), r.Edited, r.Attempts,
			r.Report.ACR*100, r.Report.TCVR*100, r.TokPerSec,
			r.Report.WCR.Seconds(), trunc(served, 18))
	}
	if c.Winner != "" {
		fmt.Fprintf(&b, "winner: %s\n  (%s)\n", c.Winner, c.Basis)
	} else {
		fmt.Fprintf(&b, "winner: none — %s\n", c.Basis)
	}
	return b.String()
}

func trunc(s string, n int) string {
	if len(s) <= n {
		return s
	}
	return s[:n-1] + "…"
}
