// Package profile is the background model profiler (PROFILE-001): it probes the host — available
// memory, memory BANDWIDTH, cores, accelerator — and computes, for each origin-allowed catalog
// model, whether it "fits" under three gates (capacity, throughput, headroom) and how fast it would
// decode, so aegis can recommend the largest model the host can actually run well. It is pure +
// read-only: no downloads, no serving, no calibration mutation. See docs/requirements/model-profiler.md.
//
// "Fits" is not a RAM check. Decode is memory-bandwidth-bound (tok/s ≈ bandwidth ÷ active-bytes/token),
// which is why the same model is interactive on a 600 GB/s Mac and a slideshow on a 50 GB/s DDR4 box —
// so we probe bandwidth, not just capacity. The per-model params/KV figures here are estimates derived
// from catalog size + id; the authoritative tok/s comes later from a micro-bench (deferred).
package profile

import (
	"os"
	"regexp"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/rtmx-ai/aegis-cli/internal/install"
)

// HostProfile is the measured host: capacity (total + available-now memory), the throughput gate
// (memory bandwidth), and the compute context (cores, accelerator, serving target).
type HostProfile struct {
	Target            string `json:"target"`
	OS                string `json:"os"`
	PhysicalCPU       int    `json:"physical_cpu"`
	TotalRAMBytes     uint64 `json:"total_ram_bytes"`
	AvailableRAMBytes uint64 `json:"available_ram_bytes"`
	MemBandwidthBps   uint64 `json:"mem_bandwidth_bytes_per_sec"`
	Accel             string `json:"accel"`
}

// ModelSpec is the catalog facts the fit needs: the GGUF byte size (exact weights), the file name
// (carries the quant), and the model id (carries the active-params hint for MoE) + origin.
type ModelSpec struct {
	ID        string `json:"id"`
	File      string `json:"file"`
	SizeBytes uint64 `json:"size"`
	Origin    string `json:"origin"`
	// ActiveParams is the MoE active-parameter count per token, when the catalog states it. It is
	// authoritative for throughput (bytes-read/token); 0 means derive from the id's `aNb` hint, else
	// treat as dense (active = total). Use it for MoE models whose id carries no active-param hint.
	ActiveParams uint64 `json:"active_params"`
}

// Floors are the per-mode "acceptable tok/s" bars: interactive (the TUI) is higher than unattended
// (aegis run / loop), so a bigger model can "fit" headless that you'd never tolerate interactively.
type Floors struct {
	InteractiveTokPerSec float64 `json:"interactive_tok_per_sec"`
	UnattendedTokPerSec  float64 `json:"unattended_tok_per_sec"`
}

// DefaultFloors are conservative interactive/unattended bars (tok/s). Tunable as we calibrate.
func DefaultFloors() Floors { return Floors{InteractiveTokPerSec: 10, UnattendedTokPerSec: 3} }

// ModelFit is the verdict for one model on this host.
type ModelFit struct {
	ID                 string  `json:"id"`
	OriginAllowed      bool    `json:"origin_allowed"`
	RequiredBytes      uint64  `json:"required_bytes"`
	FitsCapacity       bool    `json:"fits_capacity"`
	PredictedTokPerSec float64 `json:"predicted_tok_per_sec"`
	// Measured is true when PredictedTokPerSec was replaced by a real micro-bench (authoritative).
	Measured        bool `json:"measured"`
	FitsInteractive bool `json:"fits_interactive"`
	FitsUnattended  bool `json:"fits_unattended"`
}

// Recommendation is the profiler output: the probed host, every allowed model's fit (largest-first),
// and the best id for each mode (the largest model that clears that mode's floor), or "" if none.
type Recommendation struct {
	Profile     HostProfile `json:"profile"`
	CtxTokens   int         `json:"ctx_tokens"`
	Floors      Floors      `json:"floors"`
	Fits        []ModelFit  `json:"fits"`
	Interactive string      `json:"interactive_pick"`
	Unattended  string      `json:"unattended_pick"`
}

// Tuning constants — documented estimates; the micro-bench (deferred) is the authoritative confirm.
const (
	gib = uint64(1) << 30
	// reserveBytes holds memory back for the OS + co-located sibling workers (the harness, rtmx, the
	// verify phase) that are NOT running while `aegis profile` measures AvailableRAM but WILL run
	// during use. Gate 3 (headroom).
	reserveBytes = 3 * gib
	// computeBufferBytes is a flat allowance for llama.cpp's compute/scratch buffers on top of
	// weights + KV.
	computeBufferBytes = 1 * gib
	// bandwidthEfficiency derates probed memory bandwidth to what llama.cpp sustains in decode
	// (it does more than pure streaming reads). The ratio matters more than the absolute for ranking.
	bandwidthEfficiency = 0.65
	// kvBytesPerBillionParams scales the KV-cache-per-token estimate with model size (bigger models →
	// more layers/heads). ≈ 7 KB/token per billion params at FP16 KV (a 26B model ≈ 182 KB/token).
	kvBytesPerBillionParams = 7000.0
)

// Probe measures the host. It reuses internal/install.Detect for the static facts (cores, total RAM,
// accelerator, target) and adds the two dynamic gates: available-now RAM and memory bandwidth.
func Probe() HostProfile {
	caps := install.Detect()
	target := "linux-cpu"
	if caps.OS == "darwin" {
		target = "darwin-metal"
	}
	avail := availableRAMLinux()
	if avail == 0 || avail > caps.TotalRAMBytes {
		avail = caps.TotalRAMBytes // darwin / probe failure: fall back to total (refined later)
	}
	return HostProfile{
		Target:            target,
		OS:                caps.OS,
		PhysicalCPU:       caps.PhysicalCPU,
		TotalRAMBytes:     caps.TotalRAMBytes,
		AvailableRAMBytes: avail,
		MemBandwidthBps:   probeBandwidthBps(),
		Accel:             string(caps.Accel),
	}
}

// availableRAMLinux reads MemAvailable from /proc/meminfo (memory that can be given to a new workload
// without swapping — i.e. total minus what the OS + current processes truly need). 0 on non-linux.
func availableRAMLinux() uint64 {
	b, err := os.ReadFile("/proc/meminfo")
	if err != nil {
		return 0
	}
	for _, line := range strings.Split(string(b), "\n") {
		if strings.HasPrefix(line, "MemAvailable:") {
			f := strings.Fields(line)
			if len(f) >= 2 {
				if kb, err := strconv.ParseUint(f[1], 10, 64); err == nil {
					return kb * 1024
				}
			}
		}
	}
	return 0
}

// probeBandwidthBps measures achievable main-memory read bandwidth with a multi-core, cache-busting
// sweep: each worker touches one byte per 64-byte cache line over a buffer far larger than L3, so the
// CPU fetches every line from DRAM. Returns bytes/sec (0 if it can't measure).
func probeBandwidthBps() uint64 {
	const bufBytes = 64 << 20 // 64 MiB/worker — well past L3
	const iters = 8
	workers := install.Detect().PhysicalCPU
	if workers < 1 {
		workers = 1
	}
	if workers > 8 {
		workers = 8
	}
	bufs := make([][]byte, workers)
	for i := range bufs {
		bufs[i] = make([]byte, bufBytes)
		for j := 0; j < bufBytes; j += 4096 {
			bufs[i][j] = byte(j) // fault the pages in before timing
		}
	}
	sinks := make([]uint64, workers)
	var wg sync.WaitGroup
	start := time.Now()
	for w := 0; w < workers; w++ {
		wg.Add(1)
		go func(w int) {
			defer wg.Done()
			b := bufs[w]
			var s uint64
			for it := 0; it < iters; it++ {
				for k := 0; k < len(b); k += 64 {
					s += uint64(b[k])
				}
			}
			sinks[w] = s
		}(w)
	}
	wg.Wait()
	elapsed := time.Since(start).Seconds()
	_ = sinks // keep the reads live
	if elapsed <= 0 {
		return 0
	}
	total := float64(workers) * float64(bufBytes) * float64(iters)
	return uint64(total / elapsed)
}

var activeParamRe = regexp.MustCompile(`a(\d+(?:\.\d+)?)b`)

// bytesPerParam maps the quant in a GGUF file name to bytes per weight (≈ effective bits/8). Defaults
// to Q4 when unrecognized.
func bytesPerParam(file string) float64 {
	f := strings.ToLower(file)
	switch {
	case strings.Contains(f, "f16"), strings.Contains(f, "bf16"):
		return 2.0
	case strings.Contains(f, "q8"):
		return 1.06
	case strings.Contains(f, "q6"):
		return 0.82
	case strings.Contains(f, "q5"):
		return 0.71
	default: // q4 and unknown
		return 0.58
	}
}

// deriveParams estimates total + active parameter counts. Total comes from the exact GGUF size ÷
// bytes-per-param (robust); active comes from the MoE "aNb" hint in the id (e.g. a4b → 4e9), falling
// back to total for dense models.
func deriveParams(id string, sizeBytes uint64, bpp float64) (total, active uint64) {
	if bpp <= 0 {
		bpp = 0.58
	}
	total = uint64(float64(sizeBytes) / bpp)
	active = total
	if m := activeParamRe.FindStringSubmatch(strings.ToLower(id)); m != nil {
		if b, err := strconv.ParseFloat(m[1], 64); err == nil && b > 0 {
			active = uint64(b * 1e9)
		}
	}
	return total, active
}

// kvBytes estimates KV-cache bytes at a context length, scaling per-token KV with model size.
func kvBytes(totalParams uint64, ctxTokens int) uint64 {
	if ctxTokens <= 0 {
		ctxTokens = 4096
	}
	perToken := kvBytesPerBillionParams * (float64(totalParams) / 1e9)
	return uint64(perToken * float64(ctxTokens))
}

// Fit computes one model's verdict on the host: capacity (weights + KV(ctx) + buffers ≤ available −
// reserve) and roofline throughput (effective bandwidth ÷ active bytes/token) against the floors.
func Fit(spec ModelSpec, p HostProfile, ctxTokens int, f Floors) ModelFit {
	bpp := bytesPerParam(spec.File)
	total, active := deriveParams(spec.ID, spec.SizeBytes, bpp)
	if spec.ActiveParams > 0 {
		active = spec.ActiveParams // explicit catalog active_params — authoritative for MoE
	}

	required := spec.SizeBytes + kvBytes(total, ctxTokens) + computeBufferBytes
	usable := uint64(0)
	if p.AvailableRAMBytes > reserveBytes {
		usable = p.AvailableRAMBytes - reserveBytes
	}
	fitsCap := required <= usable

	bytesPerTok := float64(active) * bpp
	tps := 0.0
	if bytesPerTok > 0 && p.MemBandwidthBps > 0 {
		tps = (float64(p.MemBandwidthBps) * bandwidthEfficiency) / bytesPerTok
	}

	return ModelFit{
		ID:                 spec.ID,
		RequiredBytes:      required,
		FitsCapacity:       fitsCap,
		PredictedTokPerSec: tps,
		FitsInteractive:    fitsCap && tps >= f.InteractiveTokPerSec,
		FitsUnattended:     fitsCap && tps >= f.UnattendedTokPerSec,
	}
}

// Recommend ranks the origin-allowed models largest-first and picks, per mode, the largest model that
// clears that mode's floor. allowed reports whether a model's origin is permitted (the US-only policy).
func Recommend(specs []ModelSpec, allowed func(origin string) bool, p HostProfile, ctxTokens int, f Floors) Recommendation {
	rec := Recommendation{Profile: p, CtxTokens: ctxTokens, Floors: f}
	// Largest-first: more parameters ≈ more capable, so prefer the biggest that fits.
	ordered := append([]ModelSpec(nil), specs...)
	sortBySizeDesc(ordered)
	for _, s := range ordered {
		ok := allowed(s.Origin)
		fit := Fit(s, p, ctxTokens, f)
		fit.OriginAllowed = ok
		if !ok {
			continue // never recommend or list a disallowed-origin model
		}
		rec.Fits = append(rec.Fits, fit)
		if rec.Interactive == "" && fit.FitsInteractive {
			rec.Interactive = fit.ID
		}
		if rec.Unattended == "" && fit.FitsUnattended {
			rec.Unattended = fit.ID
		}
	}
	return rec
}

func sortBySizeDesc(s []ModelSpec) {
	for i := 1; i < len(s); i++ {
		for j := i; j > 0 && s[j-1].SizeBytes < s[j].SizeBytes; j-- {
			s[j-1], s[j] = s[j], s[j-1]
		}
	}
}
