package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"os"
	"path/filepath"

	"github.com/rtmx-ai/aegis-cli/internal/origin"
	"github.com/rtmx-ai/aegis-cli/internal/profile"
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
	if err := fs.Parse(args); err != nil {
		return 2
	}

	specs, err := catalogModelSpecs()
	if err != nil {
		fmt.Fprintf(stderr, "aegis: profile: %v\n", err)
		return 1
	}
	allowed := func(string) bool { return true }
	if pol, perr := origin.LoadPolicy(originPolicyPath()); perr == nil {
		allowed = pol.Allows
	}

	rec := profile.Recommend(specs, allowed, profile.Probe(), *ctx, profile.DefaultFloors())

	// Cache the recommendation (best-effort; never fatal).
	if home, herr := os.UserHomeDir(); herr == nil {
		dir := filepath.Join(home, ".config", "aegis")
		if os.MkdirAll(dir, 0o755) == nil {
			if b, merr := json.MarshalIndent(rec, "", "  "); merr == nil {
				_ = os.WriteFile(filepath.Join(dir, "profile.json"), b, 0o644)
			}
		}
	}

	if *asJSON {
		b, _ := json.MarshalIndent(rec, "", "  ")
		fmt.Fprintln(stdout, string(b))
		return 0
	}
	printProfile(stdout, rec)
	return 0
}

// catalogModelSpecs parses the model catalog into the fields the profiler needs.
func catalogModelSpecs() ([]profile.ModelSpec, error) {
	b, err := deployFileBytes("deploy/models/catalog.json")
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
	for _, f := range rec.Fits {
		fmt.Fprintf(w, "%-26s %4dGB %6s %8.1f  %-11s %-10s\n",
			f.ID, f.RequiredBytes>>30, yesno(f.FitsCapacity), f.PredictedTokPerSec,
			yesno(f.FitsInteractive), yesno(f.FitsUnattended))
	}
	fmt.Fprintf(w, "\nrecommendation: interactive → %s   unattended → %s\n",
		dashIfEmpty(rec.Interactive), dashIfEmpty(rec.Unattended))
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
