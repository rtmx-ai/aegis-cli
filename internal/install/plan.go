package install

import (
	"fmt"

	"github.com/rtmx-ai/aegis-cli/internal/config"
	"github.com/rtmx-ai/aegis-cli/internal/serving"
)

// ModelTier names the recommended model envelope for a host, tied to the
// sizing table in docs/hardware-purchase-spec.md. Tiers are ordered by the
// largest model the host can hold responsively at Q4_K_M:
//
//	26B-A4B  ≈ 14 GB  — the small-active MoE; fits the minimum envelope.
//	35B-A3B  ≈ 20 GB  — both MoEs; the mid envelope.
//	larger   ≥ 40 GB  — 70B-class and up; high-memory headroom.
type ModelTier string

// Model tiers, smallest to largest.
const (
	TierSmall ModelTier = "26B-A4B" // ~14 GB Q4_K_M
	TierMid   ModelTier = "35B-A3B" // ~20 GB Q4_K_M
	TierLarge ModelTier = "larger"  // 70B-class+ (~40 GB+)
)

// InstallPlan is the recommendation Plan derives from a HostCaps: the serving
// target, the model tier the host can hold, a calibration seed for bench.sh to
// refine, and an offline-safe Config the installer writes. The plan is advisory
// for tier (a human still chooses the GGUF) and authoritative for target +
// calibration shape.
type InstallPlan struct {
	// Caps is the detected host, echoed for the summary.
	Caps HostCaps
	// Target is the recommended serving target.
	Target config.Target
	// Tier is the recommended model envelope (see ModelTier).
	Tier ModelTier
	// Calibration is a SEED calibration in the shape internal/serving expects.
	// It is not a measured result: scripts/bench.sh replaces it with the
	// host-tuned winner. Model is left as a placeholder for the operator.
	Calibration serving.Calibration
	// Config is the offline-safe orchestrator config to write (AllowEgress
	// false; loopback endpoint). Target mirrors Plan.Target.
	Config config.Config
	// Notes are human-readable caveats surfaced in the summary (e.g. an
	// unrecognized OS defaulting to linux-cpu, or a missing accelerator).
	Notes []string
}

// seedPort is the default loopback port for the serving endpoint and the
// calibration seed. It matches config.Default()'s endpoint port.
const seedPort = 8080

// Plan maps host capabilities to an install recommendation. It is a pure
// function: the same HostCaps always yields the same InstallPlan, with no I/O.
//
// Target selection: darwin → darwin-metal (all layers on Metal, -ngl 999);
// everything else → linux-cpu (CPU-only, -ngl 0, threads pinned to physical
// cores). The Mac path is wired and waiting per CLAUDE.md §2.
//
// Tier selection is by usable inference memory (RAM/unified) against the
// Q4_K_M footprints in docs/hardware-purchase-spec.md, leaving headroom for KV
// cache and the OS:
//
//	< 24 GiB → 26B-A4B (small); < 56 GiB → 35B-A3B (mid); ≥ 56 GiB → larger.
func Plan(caps HostCaps) InstallPlan {
	cfg := config.Default()
	cfg.AllowEgress = false // offline-safe, explicit.

	var notes []string

	target := config.TargetLinuxCPU
	switch caps.OS {
	case "darwin":
		target = config.TargetDarwinMetal
	case "linux":
		target = config.TargetLinuxCPU
	default:
		notes = append(notes, fmt.Sprintf("unrecognized OS %q; defaulting to linux-cpu target", caps.OS))
	}
	cfg.Target = target

	tier := pickTier(caps.TotalRAMBytes)
	if caps.TotalRAMBytes == 0 {
		notes = append(notes, "RAM probe returned 0; defaulting to the small (26B-A4B) tier")
	}

	cal := seedCalibration(target, caps)

	// Accelerator advisory: linux-cpu is correct even with a GPU present (the
	// stack's linux target is CPU-only by design), but surface the mismatch so
	// the operator can choose a GPU build deliberately if they have one.
	switch {
	case target == config.TargetLinuxCPU && caps.Accel == AccelNVIDIA:
		notes = append(notes, "NVIDIA GPU detected; linux target serves CPU-only (-ngl 0) by design — build a CUDA llama.cpp deliberately if you want offload")
	case target == config.TargetLinuxCPU && caps.Accel == AccelROCm:
		notes = append(notes, "AMD/ROCm GPU detected; linux target serves CPU-only (-ngl 0) by design — build a ROCm llama.cpp deliberately if you want offload")
	}

	return InstallPlan{
		Caps:        caps,
		Target:      target,
		Tier:        tier,
		Calibration: cal,
		Config:      cfg,
		Notes:       notes,
	}
}

// pickTier maps total RAM (bytes) to a model tier using the Q4_K_M footprints
// from docs/hardware-purchase-spec.md plus headroom for KV cache and the OS.
func pickTier(ramBytes uint64) ModelTier {
	const gib = uint64(1) << 30
	switch {
	case ramBytes >= 56*gib:
		return TierLarge // room for 70B-class (~40 GB) + context.
	case ramBytes >= 24*gib:
		return TierMid // room for 35B-A3B (~20 GB) + context.
	default:
		return TierSmall // 26B-A4B (~14 GB) minimum envelope.
	}
}

// seedCalibration builds a per-target calibration seed that validates under
// internal/serving's rules. linux-cpu: threads ≈ physical cores, -ngl 0.
// darwin-metal: -ngl 999 (all layers), threads not the lever. Model is a
// placeholder for the operator to fill; bench.sh overwrites the whole file.
func seedCalibration(target config.Target, caps HostCaps) serving.Calibration {
	const modelPlaceholder = "/models/REPLACE-ME.gguf"
	switch target {
	case config.TargetDarwinMetal:
		return serving.Calibration{
			Target:  serving.TargetDarwinMetal,
			Threads: 0, // GPU does the work; not the lever on Metal.
			Batch:   512,
			NGL:     999, // all layers offloaded.
			Model:   modelPlaceholder,
			Port:    seedPort,
		}
	default:
		return serving.Calibration{
			Target:  serving.TargetLinuxCPU,
			Threads: max1(caps.PhysicalCPU), // pin to physical cores.
			Batch:   512,
			NGL:     0, // CPU-only.
			Model:   modelPlaceholder,
			Port:    seedPort,
		}
	}
}
