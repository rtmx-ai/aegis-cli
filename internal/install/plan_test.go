package install

import (
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/config"
	"github.com/rtmx-ai/aegis-cli/internal/serving"
)

const gib = uint64(1) << 30

// TestPlanMapsCapsToTargetTierCalibration → INSTALL: Plan maps representative
// hosts to the expected target, model tier, and calibration seed (table-driven:
// a Ryzen-like linux box → linux-cpu; an M-series-like darwin box → darwin-metal).
func TestPlanMapsCapsToTargetTierCalibration(t *testing.T) {
	cases := []struct {
		name        string
		caps        HostCaps
		wantTarget  config.Target
		wantTier    ModelTier
		wantThreads int
		wantNGL     int
	}{
		{
			name: "ryzen-linux-cpu",
			caps: HostCaps{
				OS: "linux", Arch: "amd64",
				LogicalCPU: 32, PhysicalCPU: 16,
				TotalRAMBytes: 64 * gib, Accel: AccelNone,
			},
			wantTarget:  config.TargetLinuxCPU,
			wantTier:    TierLarge, // 64 GiB >= 56 GiB
			wantThreads: 16,        // pinned to physical cores
			wantNGL:     0,         // CPU-only
		},
		{
			name: "m5max-darwin-metal",
			caps: HostCaps{
				OS: "darwin", Arch: "arm64",
				LogicalCPU: 16, PhysicalCPU: 12,
				TotalRAMBytes: 128 * gib, Accel: AccelMetal,
			},
			wantTarget:  config.TargetDarwinMetal,
			wantTier:    TierLarge,
			wantThreads: 0,   // not the lever on Metal
			wantNGL:     999, // all layers offloaded
		},
		{
			name: "small-linux-box",
			caps: HostCaps{
				OS: "linux", Arch: "amd64",
				LogicalCPU: 8, PhysicalCPU: 4,
				TotalRAMBytes: 16 * gib, Accel: AccelNone,
			},
			wantTarget:  config.TargetLinuxCPU,
			wantTier:    TierSmall, // 16 GiB < 24 GiB
			wantThreads: 4,
			wantNGL:     0,
		},
		{
			name: "mid-linux-box",
			caps: HostCaps{
				OS: "linux", Arch: "amd64",
				LogicalCPU: 16, PhysicalCPU: 8,
				TotalRAMBytes: 32 * gib, Accel: AccelNone,
			},
			wantTarget:  config.TargetLinuxCPU,
			wantTier:    TierMid, // 24 <= 32 < 56
			wantThreads: 8,
			wantNGL:     0,
		},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			p := Plan(tc.caps)
			if p.Target != tc.wantTarget {
				t.Errorf("target = %s, want %s", p.Target, tc.wantTarget)
			}
			if p.Config.Target != tc.wantTarget {
				t.Errorf("config target = %s, want %s", p.Config.Target, tc.wantTarget)
			}
			if p.Tier != tc.wantTier {
				t.Errorf("tier = %s, want %s", p.Tier, tc.wantTier)
			}
			if p.Calibration.Threads != tc.wantThreads {
				t.Errorf("threads = %d, want %d", p.Calibration.Threads, tc.wantThreads)
			}
			if p.Calibration.NGL != tc.wantNGL {
				t.Errorf("ngl = %d, want %d", p.Calibration.NGL, tc.wantNGL)
			}
			// The seed must validate under config and serving rules.
			if err := config.Validate(p.Config); err != nil {
				t.Errorf("planned config must validate: %v", err)
			}
			if p.Config.AllowEgress {
				t.Error("planned config must be offline-safe (AllowEgress false)")
			}
		})
	}
}

// TestPlanCalibrationSeedRoundTripsServing → INSTALL: the calibration seed
// validates and launches under internal/serving for its target.
func TestPlanCalibrationSeedRoundTripsServing(t *testing.T) {
	for _, caps := range []HostCaps{
		{OS: "linux", PhysicalCPU: 16, LogicalCPU: 32, TotalRAMBytes: 64 * gib},
		{OS: "darwin", PhysicalCPU: 12, LogicalCPU: 16, TotalRAMBytes: 128 * gib},
	} {
		p := Plan(caps)
		if _, err := serving.LaunchArgs(&p.Calibration); err != nil {
			t.Errorf("%s: seed calibration must produce launch args: %v", caps.OS, err)
		}
	}
}

// TestPlanUnknownOSDefaultsLinuxCPU → INSTALL: an unrecognized OS defaults to the
// linux-cpu target and records a note.
func TestPlanUnknownOSDefaultsLinuxCPU(t *testing.T) {
	p := Plan(HostCaps{OS: "plan9", Arch: "amd64", LogicalCPU: 4, PhysicalCPU: 2})
	if p.Target != config.TargetLinuxCPU {
		t.Errorf("target = %s, want linux-cpu", p.Target)
	}
	if len(p.Notes) == 0 {
		t.Error("expected a note about the unrecognized OS")
	}
}

// TestPlanRAMZeroDefaultsSmallTier → INSTALL: a failed RAM probe (0 bytes)
// defaults to the small tier with a note.
func TestPlanRAMZeroDefaultsSmallTier(t *testing.T) {
	p := Plan(HostCaps{OS: "linux", LogicalCPU: 8, PhysicalCPU: 8, TotalRAMBytes: 0})
	if p.Tier != TierSmall {
		t.Errorf("tier = %s, want %s on zero RAM", p.Tier, TierSmall)
	}
}
