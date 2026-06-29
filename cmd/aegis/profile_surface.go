package main

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"

	"github.com/rtmx-ai/aegis-cli/internal/origin"
	"github.com/rtmx-ai/aegis-cli/internal/profile"
)

// computeRecommendation probes the host + ranks the origin-allowed catalog models (no micro-bench).
func computeRecommendation(ctxTokens int) (profile.Recommendation, error) {
	specs, err := catalogModelSpecs()
	if err != nil {
		return profile.Recommendation{}, err
	}
	allowed := func(string) bool { return true }
	if pol, perr := origin.LoadPolicy(originPolicyPath()); perr == nil {
		allowed = pol.Allows
	}
	return profile.Recommend(specs, allowed, profile.Probe(), ctxTokens, profile.DefaultFloors()), nil
}

func profileCachePath() string {
	home, err := os.UserHomeDir()
	if err != nil {
		return ""
	}
	return filepath.Join(home, ".config", "aegis", "profile.json")
}

func writeProfileCache(rec profile.Recommendation) error {
	p := profileCachePath()
	if p == "" {
		return fmt.Errorf("no home dir")
	}
	if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
		return err
	}
	b, err := json.MarshalIndent(rec, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(p, b, 0o644)
}

func loadProfileCache() (*profile.Recommendation, error) {
	p := profileCachePath()
	if p == "" {
		return nil, fmt.Errorf("no home dir")
	}
	b, err := os.ReadFile(p)
	if err != nil {
		return nil, err
	}
	var rec profile.Recommendation
	if err := json.Unmarshal(b, &rec); err != nil {
		return nil, err
	}
	return &rec, nil
}

// autoProfile runs the profiler once + caches it (PROFILE-003, first-launch profiling). No-op if a
// cache already exists (cheap re-launch) or the catalog is unavailable. The probe is ~0.3s and runs
// after the model is already up, so it never delays getting the operator into the TUI.
func autoProfile() {
	if _, err := loadProfileCache(); err == nil {
		return // already profiled this host
	}
	rec, err := computeRecommendation(16384)
	if err != nil {
		return
	}
	_ = writeProfileCache(rec)
}

// profileHint returns a one-line, non-intrusive surfacing of the cached profile for the launch banner:
// the best-fitting US model for this host, plus an upgrade nudge when the running model isn't it.
// Empty when no profile is cached yet.
func profileHint(runningModel string) string {
	rec, err := loadProfileCache()
	if err != nil || rec == nil || len(rec.Fits) == 0 {
		return ""
	}
	best := rec.Interactive
	if best == "" {
		best = rec.Unattended
	}
	if best == "" {
		return ""
	}
	if runningModel != "" && runningModel != best {
		return fmt.Sprintf("this host fits %s best — you're running %s (run `aegis profile` for details)", best, runningModel)
	}
	return fmt.Sprintf("best-fitting model for this host: %s (run `aegis profile` for the full table)", best)
}
