package main

import (
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/rtmx-ai/aegis-cli/internal/config"
	"github.com/rtmx-ai/aegis-cli/internal/mockmodel"
)

// TestEnsureModelServingAlreadyUp covers the early-return path: when a model already answers on the
// endpoint, ensureModelServing opens the TUI against it without starting a second server.
func TestEnsureModelServingAlreadyUp(t *testing.T) {
	srv := mockmodel.New(mockmodel.Options{Responses: []mockmodel.Response{{Content: "pong"}}})
	defer srv.Close()
	cfg := config.Default()
	cfg.Endpoint = srv.URL()
	stop, _, err := ensureModelServing(cfg, io.Discard)
	if err != nil {
		t.Fatalf("an already-serving endpoint must not error: %v", err)
	}
	if stop != nil {
		stop()
		t.Error("no server should be started when one already serves")
	}
}

// TestModelAutoServeResourceAware → REQ-OC-023: bare `aegis` brings up a model before the TUI, and
// the recommended/served model is resource-aware (internal/install Plan picks the envelope the host
// can hold) — not every model fits every system. Live auto-serve is covered by the gated smoke;
// here we cover the guidance, calibration resolution, readiness probe, and the no-model error path.
func TestModelAutoServeResourceAware(t *testing.T) {
	g := provisionGuidance()
	if !strings.Contains(g, "not every model fits every system") {
		t.Error("provision guidance must state the resource-aware principle")
	}
	if !strings.Contains(g, "26B-A4B") && !strings.Contains(g, "35B-A3B") && !strings.Contains(g, "larger") {
		t.Errorf("provision guidance must name the host's resource-aware tier:\n%s", g)
	}
	t.Setenv("AEGIS_CALIBRATION", "/tmp/aegis-test/calibration.json")
	if got := resolveCalibrationPath(); got != "/tmp/aegis-test/calibration.json" {
		t.Errorf("AEGIS_CALIBRATION override = %q, want the explicit path", got)
	}
}

func TestEndpointReadyDead(t *testing.T) {
	if endpointReady("http://127.0.0.1:1", 2*time.Second) {
		t.Error("a dead endpoint must not report ready")
	}
}

// ensureModelServing must error (not start a server) when the calibrated model is absent.
func TestEnsureModelServingMissingModel(t *testing.T) {
	cfg := config.Default()
	cfg.Endpoint = "http://127.0.0.1:1" // reliably dead → no model serving
	tmp := t.TempDir()
	cal := filepath.Join(tmp, "calibration.json")
	if err := os.WriteFile(cal, []byte(`{"target":"linux-cpu","threads":4,"batch":512,"ngl":0,"model":"/no/such/model.gguf","port":8080,"ctx_size":4096}`), 0o644); err != nil {
		t.Fatal(err)
	}
	t.Setenv("AEGIS_CALIBRATION", cal)
	stop, _, err := ensureModelServing(cfg, io.Discard)
	if stop != nil {
		stop()
	}
	if err == nil {
		t.Fatal("expected an error when the calibrated model is absent")
	}
	if !strings.Contains(err.Error(), "not present") && !strings.Contains(err.Error(), "provision") {
		t.Errorf("error should guide toward provisioning, got: %v", err)
	}
}

func TestCatalogIDForGGUF(t *testing.T) {
	root, err := filepath.Abs("../..")
	if err != nil {
		t.Fatal(err)
	}
	t.Chdir(root) // the catalog resolves cwd-relative from the repo root
	if id := catalogIDForGGUF("/models/gemma-4-26B-A4B-it-qat-UD-Q4_K_XL.gguf"); id != "gemma-4-26b-a4b" {
		t.Errorf("catalog id for the gemma GGUF = %q, want gemma-4-26b-a4b", id)
	}
	if id := catalogIDForGGUF("/models/not-in-catalog.gguf"); id != "" {
		t.Errorf("unknown GGUF should map to no id, got %q", id)
	}
}
