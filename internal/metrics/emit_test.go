package metrics

import (
	"encoding/json"
	"testing"
	"time"
)

func sampleCollector() *Collector {
	c := NewCollector()
	c.Record(Attempt{
		RequirementID: "A-001", Closed: true, FirstPass: true, Turns: 2,
		ToolCalls: 4, ValidToolCalls: 4, Tokens: 100, WallClock: time.Second,
		Stages: Stages{Prefill: 100 * time.Millisecond, Decode: 400 * time.Millisecond, Verify: 200 * time.Millisecond, HarnessOverhead: 50 * time.Millisecond},
	})
	c.Record(Attempt{
		RequirementID: "A-002", Closed: true, FirstPass: false, Turns: 4,
		ToolCalls: 6, ValidToolCalls: 5, Tokens: 300, WallClock: 3 * time.Second,
	})
	c.Record(Attempt{
		RequirementID: "A-003", Escalated: true, Turns: 6,
		ToolCalls: 4, ValidToolCalls: 3, Tokens: 200, WallClock: 2 * time.Second,
	})
	return c
}

func TestReportEmitsAllDashboardMetrics(t *testing.T) {
	r := sampleCollector().Report()
	if r.Attempted != 3 || r.Closed != 2 || r.Escalated != 1 {
		t.Fatalf("counts: attempted=%d closed=%d escalated=%d", r.Attempted, r.Closed, r.Escalated)
	}
	// ACR = closed/attempted = 2/3
	if got := r.ACR; got < 0.66 || got > 0.67 {
		t.Errorf("ACR = %v, want ~0.667", got)
	}
	// ESC = 1/3
	if r.ESC < 0.33 || r.ESC > 0.34 {
		t.Errorf("ESC = %v, want ~0.333", r.ESC)
	}
	// FPVR = first-pass closed / closed = 1/2
	if r.FPVR != 0.5 {
		t.Errorf("FPVR = %v, want 0.5", r.FPVR)
	}
	// TCVR = 12/14
	if r.TCVR < 0.85 || r.TCVR > 0.858 {
		t.Errorf("TCVR = %v, want ~0.857", r.TCVR)
	}
	// MTC = closed turns / closed = (2+4)/2 = 3
	if r.MTC != 3 {
		t.Errorf("MTC = %v, want 3", r.MTC)
	}
	if r.TCR == 0 || r.WCR == 0 {
		t.Errorf("TCR/WCR should be non-zero: TCR=%v WCR=%v", r.TCR, r.WCR)
	}

	b, err := r.JSON()
	if err != nil {
		t.Fatal(err)
	}
	if !json.Valid(b) {
		t.Fatal("report JSON must be valid")
	}
}

func TestReportEmptyIsZero(t *testing.T) {
	r := NewCollector().Report()
	if r.Attempted != 0 || r.ACR != 0 {
		t.Fatalf("empty report should be zero, got %+v", r)
	}
}
