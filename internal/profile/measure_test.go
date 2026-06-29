package profile

import (
	"context"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/mockmodel"
)

func TestMeasureTokPerSec(t *testing.T) {
	resp := mockmodel.Response{Content: "one two three four five six seven eight"}
	srv := mockmodel.New(mockmodel.Options{Responses: []mockmodel.Response{resp, resp, resp}})
	defer srv.Close()
	tps, err := MeasureTokPerSec(context.Background(), srv.URL(), "local", 32)
	if err != nil {
		t.Fatalf("measure: %v", err)
	}
	if tps <= 0 {
		t.Errorf("measured tok/s must be > 0, got %v", tps)
	}
}

// TestApplyMeasurement: a model that benches below its interactive floor steps down; the row is
// marked authoritative; a model still above the unattended floor keeps that pick.
func TestApplyMeasurement(t *testing.T) {
	rec := Recommendation{
		Floors: DefaultFloors(), // interactive 10, unattended 3
		Fits: []ModelFit{
			{ID: "big", FitsCapacity: true, PredictedTokPerSec: 20, FitsInteractive: true, FitsUnattended: true},
			{ID: "small", FitsCapacity: true, PredictedTokPerSec: 12, FitsInteractive: true, FitsUnattended: true},
		},
		Interactive: "big", Unattended: "big",
	}
	rec.ApplyMeasurement("big", 5) // big really decodes at 5 tok/s
	if !rec.Fits[0].Measured {
		t.Error("the benched model must be marked measured")
	}
	if rec.Fits[0].FitsInteractive {
		t.Error("5 tok/s must not clear the interactive floor (10)")
	}
	if rec.Interactive != "small" {
		t.Errorf("interactive must step down to small, got %q", rec.Interactive)
	}
	if rec.Unattended != "big" {
		t.Errorf("big still clears unattended (3); got %q", rec.Unattended)
	}
}
