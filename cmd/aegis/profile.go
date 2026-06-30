package main

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"time"

	"github.com/rtmx-ai/aegis-cli/internal/config"
	"github.com/rtmx-ai/aegis-cli/internal/profile"
	"github.com/rtmx-ai/aegis-cli/internal/serving"
)

// cmdProfile implements `aegis profile` (PROFILE-001): the background model profiler. It probes the
// host (available RAM, memory bandwidth, cores, target), computes the capacity + roofline-throughput
// fit for every origin-allowed catalog model, and emits a ranked recommendation for the interactive
// and unattended floors. Pure + read-only — no download, no serve, no calibration mutation — so it is
// safe to run in the background. Caches the result to ~/.config/aegis/profile.json.
func cmdProfile(args []string, stdout, stderr io.Writer) int {
	fs := flag.NewFlagSet("profile", flag.ContinueOnError)
	fs.SetOutput(stderr)
	ctx := fs.Int("ctx", 16384, "context length (tokens) for the KV-cache fit estimate")
	asJSON := fs.Bool("json", false, "emit the full recommendation as JSON")
	bench := fs.Bool("bench", false, "micro-bench the running model for an authoritative tok/s (needs a model serving)")
	if err := fs.Parse(args); err != nil {
		return 2
	}

	rec, err := computeRecommendation(*ctx)
	if err != nil {
		fmt.Fprintf(stderr, "aegis: profile: %v\n", err)
		return 1
	}
	// --bench: replace the running model's predicted tok/s with an authoritative measurement.
	if *bench {
		benchRunningModel(stdout, stderr, &rec)
	}
	_ = writeProfileCache(rec) // best-effort; never fatal

	if *asJSON {
		b, _ := json.MarshalIndent(rec, "", "  ")
		fmt.Fprintln(stdout, string(b))
		return 0
	}
	printProfile(stdout, rec)
	return 0
}

// benchRunningModel micro-benches the model currently serving on the configured endpoint and folds
// the authoritative tok/s into rec (re-picking the floors). No-op with a clear note if nothing serves.
func benchRunningModel(stdout, stderr io.Writer, rec *profile.Recommendation) {
	cfg, err := config.Load("")
	if err != nil {
		cfg = config.Default()
	}
	if !endpointReady(cfg.Endpoint, 10*time.Second) {
		fmt.Fprintln(stderr, "aegis: profile --bench needs a running model — start `aegis` or `aegis serve` first")
		return
	}
	id := ""
	if cp := resolveCalibrationPath(); cp != "" {
		if cal, lerr := serving.LoadCalibration(cp); lerr == nil {
			id = catalogIDForGGUF(cal.Model)
		}
	}
	model := id
	if model == "" {
		model = cfg.ModelID
	}
	fmt.Fprintf(stdout, "benchmarking the running model (%s) — generating …\n", dashIfEmpty(model))
	tps, merr := profile.MeasureTokPerSec(context.Background(), cfg.Endpoint, model, 96)
	if merr != nil || tps <= 0 {
		fmt.Fprintf(stderr, "aegis: profile --bench: measurement failed: %v\n", merr)
		return
	}
	if id != "" {
		rec.ApplyMeasurement(id, tps)
	}
	fmt.Fprintf(stdout, "measured %s: %.1f tok/s (authoritative)\n\n", dashIfEmpty(model), tps)
}

// catalogModelSpecs parses the model catalog into the fields the profiler needs.
func catalogModelSpecs() ([]profile.ModelSpec, error) {
	b, err := catalogBytes()
	if err != nil {
		return nil, fmt.Errorf("model catalog not found (deploy/models/catalog.json) — run from the repo or an aegis bundle: %w", err)
	}
	var cat struct {
		Models []profile.ModelSpec `json:"models"`
	}
	if err := json.Unmarshal(b, &cat); err != nil {
		return nil, fmt.Errorf("parse catalog: %w", err)
	}
	return cat.Models, nil
}

func printProfile(w io.Writer, rec profile.Recommendation) {
	p := rec.Profile
	fmt.Fprintf(w, "aegis profile — %s | RAM %d/%d GiB avail | %d cores | %.1f GB/s mem bandwidth | accel %s\n",
		p.Target, p.AvailableRAMBytes>>30, p.TotalRAMBytes>>30, p.PhysicalCPU,
		float64(p.MemBandwidthBps)/1e9, dashIfEmpty(p.Accel))
	fmt.Fprintf(w, "context %d tokens   floors: interactive ≥%.0f tok/s, unattended ≥%.0f tok/s\n\n",
		rec.CtxTokens, rec.Floors.InteractiveTokPerSec, rec.Floors.UnattendedTokPerSec)
	fmt.Fprintf(w, "%-26s %6s %6s %8s  %-11s %-10s\n", "model (US-origin)", "need", "fits", "~tok/s", "interactive", "unattended")
	anyMeasured := false
	for _, f := range rec.Fits {
		tps := fmt.Sprintf("%.1f", f.PredictedTokPerSec)
		if f.Measured {
			tps += "*"
			anyMeasured = true
		}
		fmt.Fprintf(w, "%-26s %4dGB %6s %8s  %-11s %-10s\n",
			f.ID, f.RequiredBytes>>30, yesno(f.FitsCapacity), tps,
			yesno(f.FitsInteractive), yesno(f.FitsUnattended))
	}
	fmt.Fprintf(w, "\nrecommendation: interactive → %s   unattended → %s\n",
		dashIfEmpty(rec.Interactive), dashIfEmpty(rec.Unattended))
	if anyMeasured {
		fmt.Fprintln(w, "* measured (authoritative); other tok/s are roofline estimates (run --bench on each to confirm)")
	}
	fmt.Fprintln(w, "(advisory — provision with scripts/fetch-model.sh <id> or scripts/pin-model.sh <gguf>; estimates, refined by bench.sh)")
}

func yesno(b bool) string {
	if b {
		return "yes"
	}
	return "no"
}

func dashIfEmpty(s string) string {
	if s == "" {
		return "—"
	}
	return s
}
