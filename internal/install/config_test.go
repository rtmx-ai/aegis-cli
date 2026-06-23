package install

import (
	"bytes"
	"os"
	"path/filepath"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/config"
)

// TestWriteConfigIsOfflineSafe → INSTALL: WriteConfig refuses an egress-enabled
// config and writes only loopback-safe configs.
func TestWriteConfigIsOfflineSafe(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "aegis.json")

	bad := config.Default()
	bad.AllowEgress = true
	if err := WriteConfig(path, bad, false); err == nil {
		t.Fatal("WriteConfig must refuse AllowEgress=true")
	}
	if _, err := os.Stat(path); !os.IsNotExist(err) {
		t.Fatal("no file should be written when refusing an egress config")
	}

	good := config.Default()
	if err := WriteConfig(path, good, false); err != nil {
		t.Fatalf("WriteConfig offline-safe config: %v", err)
	}
}

// TestWriteConfigIdempotent → INSTALL: re-running WriteConfig (with overwrite)
// yields byte-identical output, and without overwrite it refuses to clobber.
func TestWriteConfigIdempotent(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "aegis.json")
	cfg := config.Default()

	if err := WriteConfig(path, cfg, false); err != nil {
		t.Fatalf("first write: %v", err)
	}
	first, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}

	// Without overwrite, a second write must refuse (no silent clobber).
	if err := WriteConfig(path, cfg, false); err == nil {
		t.Fatal("WriteConfig without overwrite must refuse an existing file")
	}

	// With overwrite, the bytes must be identical (deterministic render).
	if err := WriteConfig(path, cfg, true); err != nil {
		t.Fatalf("overwrite write: %v", err)
	}
	second, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(first, second) {
		t.Errorf("re-render not idempotent:\nfirst=%s\nsecond=%s", first, second)
	}
}

// TestWriteConfigRoundTripsViaLoad → INSTALL: a written config round-trips
// through config.Load unchanged.
func TestWriteConfigRoundTripsViaLoad(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "aegis.json")

	// Use a planned (non-default-target) config to exercise the full struct.
	plan := Plan(HostCaps{OS: "darwin", Arch: "arm64", PhysicalCPU: 12, LogicalCPU: 16, TotalRAMBytes: 128 << 30})
	if err := WriteConfig(path, plan.Config, false); err != nil {
		t.Fatalf("write: %v", err)
	}
	loaded, err := config.Load(path)
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	if loaded != plan.Config {
		t.Errorf("round-trip mismatch:\nwrote=%+v\nread =%+v", plan.Config, loaded)
	}
}

// TestWriteConfigRequiresPath → INSTALL: an empty path is rejected.
func TestWriteConfigRequiresPath(t *testing.T) {
	if err := WriteConfig("", config.Default(), false); err == nil {
		t.Fatal("empty path must be rejected")
	}
}
