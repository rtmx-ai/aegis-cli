package profile

import (
	"context"
	"fmt"
	"time"

	"github.com/rtmx-ai/aegis-cli/internal/serving"
)

// MeasureTokPerSec micro-benches a RUNNING model on endpoint: it warms the model (a discarded call to
// load weights + caches) then times a short generation, returning the measured decode rate
// (completion tokens ÷ wall-clock). This is the authoritative tok/s that confirms — or refutes — the
// roofline prediction. maxTokens defaults to 96 (enough that decode dominates the short prompt's
// prefill). The caller is responsible for there being a model serving on endpoint.
func MeasureTokPerSec(ctx context.Context, endpoint, model string, maxTokens int) (float64, error) {
	c, err := serving.NewClient(endpoint)
	if err != nil {
		return 0, err
	}
	if maxTokens <= 0 {
		maxTokens = 96
	}
	req := serving.ChatRequest{
		Model:     model,
		Messages:  []serving.Message{{Role: "user", Content: "Write a few plain sentences about software testing."}},
		MaxTokens: maxTokens,
	}
	_, _ = c.ChatCompletion(ctx, req) // warmup: load weights + caches; result discarded
	start := time.Now()
	resp, err := c.ChatCompletion(ctx, req)
	elapsed := time.Since(start).Seconds()
	if err != nil {
		return 0, err
	}
	n := resp.Usage.CompletionTokens
	if n <= 0 || elapsed <= 0 {
		return 0, fmt.Errorf("no tokens measured (completion_tokens=%d)", n)
	}
	return float64(n) / elapsed, nil
}

// ApplyMeasurement replaces a model's predicted tok/s with a measured value, marks it authoritative,
// re-evaluates that model's floors, and re-picks the interactive/unattended recommendations — so a
// model that benched below its floor steps down to the next-best. Other models keep their estimates.
func (r *Recommendation) ApplyMeasurement(modelID string, measuredTps float64) {
	for i := range r.Fits {
		if r.Fits[i].ID == modelID {
			r.Fits[i].PredictedTokPerSec = measuredTps
			r.Fits[i].Measured = true
			r.Fits[i].FitsInteractive = r.Fits[i].FitsCapacity && measuredTps >= r.Floors.InteractiveTokPerSec
			r.Fits[i].FitsUnattended = r.Fits[i].FitsCapacity && measuredTps >= r.Floors.UnattendedTokPerSec
		}
	}
	r.Interactive, r.Unattended = "", ""
	for _, f := range r.Fits { // Fits is already largest-first
		if r.Interactive == "" && f.FitsInteractive {
			r.Interactive = f.ID
		}
		if r.Unattended == "" && f.FitsUnattended {
			r.Unattended = f.ID
		}
	}
}
