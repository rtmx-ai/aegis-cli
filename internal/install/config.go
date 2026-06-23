package install

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"

	"github.com/rtmx-ai/aegis-cli/internal/config"
)

// WriteConfig writes cfg to path as JSON. It is offline-safe and idempotent:
//
//   - It refuses to write a config with AllowEgress=true (egress is
//     build-failing; the installer never persists an egress-enabled config).
//   - It validates via config.Validate before touching disk, so a malformed
//     config never lands.
//   - Without overwrite it never clobbers an existing file: it returns an error
//     so a re-run cannot silently change a hand-tuned config. With overwrite the
//     rendered bytes are deterministic, so re-running yields identical output.
//
// The written file round-trips through config.Load.
func WriteConfig(path string, cfg config.Config, overwrite bool) error {
	if path == "" {
		return fmt.Errorf("install: config path must be set")
	}
	if cfg.AllowEgress {
		return fmt.Errorf("install: refusing to write a config with AllowEgress=true (egress is build-failing)")
	}
	if err := config.Validate(cfg); err != nil {
		return fmt.Errorf("install: refusing to write invalid config: %w", err)
	}
	if !overwrite {
		if _, err := os.Stat(path); err == nil {
			return fmt.Errorf("install: config %s already exists (pass --force to overwrite)", path)
		} else if !os.IsNotExist(err) {
			return fmt.Errorf("install: stat %s: %w", path, err)
		}
	}
	data, err := renderConfig(cfg)
	if err != nil {
		return err
	}
	if dir := filepath.Dir(path); dir != "" && dir != "." {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			return fmt.Errorf("install: mkdir %s: %w", dir, err)
		}
	}
	if err := os.WriteFile(path, data, 0o600); err != nil {
		return fmt.Errorf("install: write %s: %w", path, err)
	}
	return nil
}

// renderConfig serializes cfg to deterministic, indented JSON with a trailing
// newline. Determinism (stable field order via the struct, fixed indent) is
// what makes WriteConfig idempotent.
func renderConfig(cfg config.Config) ([]byte, error) {
	data, err := json.MarshalIndent(cfg, "", "  ")
	if err != nil {
		return nil, fmt.Errorf("install: render config: %w", err)
	}
	return append(data, '\n'), nil
}

// WritePlan renders a human-readable summary of an InstallPlan to w: the
// detected caps, the chosen target/tier, the calibration seed, the resolved
// config, any notes, and next-step guidance. It performs no I/O beyond writing
// to w, so callers control where the summary goes (stdout, a dry-run buffer).
func WritePlan(w io.Writer, p InstallPlan, cfgPath string, dryRun bool) error {
	fmt.Fprintln(w, "aegis init — host bootstrap plan")
	fmt.Fprintln(w)
	fmt.Fprintln(w, "detected host:")
	fmt.Fprintf(w, "  os/arch     : %s/%s\n", p.Caps.OS, p.Caps.Arch)
	fmt.Fprintf(w, "  cpu         : %d physical / %d logical\n", p.Caps.PhysicalCPU, p.Caps.LogicalCPU)
	fmt.Fprintf(w, "  ram         : %d GiB\n", p.Caps.TotalRAMGB())
	fmt.Fprintf(w, "  accelerator : %s\n", p.Caps.Accel)
	fmt.Fprintln(w)
	fmt.Fprintln(w, "recommendation:")
	fmt.Fprintf(w, "  target      : %s\n", p.Target)
	fmt.Fprintf(w, "  model tier  : %s\n", p.Tier)
	fmt.Fprintf(w, "  calibration : threads=%d batch=%d ngl=%d port=%d (seed; bench.sh refines)\n",
		p.Calibration.Threads, p.Calibration.Batch, p.Calibration.NGL, p.Calibration.Port)
	fmt.Fprintf(w, "  endpoint    : %s (loopback; egress off)\n", p.Config.Endpoint)
	fmt.Fprintf(w, "  harness     : %s\n", p.Config.Harness)
	for _, n := range p.Notes {
		fmt.Fprintf(w, "  note        : %s\n", n)
	}
	fmt.Fprintln(w)
	if dryRun {
		fmt.Fprintf(w, "dry-run: no files written (would write config to %s)\n", cfgPath)
	} else {
		fmt.Fprintf(w, "wrote config: %s\n", cfgPath)
	}
	fmt.Fprintln(w)
	fmt.Fprintln(w, "next steps:")
	fmt.Fprintf(w, "  1. calibrate serving to this host : scripts/bench.sh --model %s\n", p.Calibration.Model)
	fmt.Fprintln(w, "  2. install git hooks (CI parity)  : make hooks-install")
	fmt.Fprintln(w, "  3. prove the enclave is closed    : scripts/verify-airgap.sh -- aegis run --once")
	return nil
}
