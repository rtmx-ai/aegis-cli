package config

import (
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestDefaultIsOfflineSafe(t *testing.T) {
	c := Default()
	if c.AllowEgress {
		t.Fatal("default AllowEgress must be false")
	}
	if err := Validate(c); err != nil {
		t.Fatalf("default config must validate: %v", err)
	}
}

func TestValidateRejectsNonLoopback(t *testing.T) {
	c := Default()
	c.Endpoint = "http://example.com:8080"
	if err := Validate(c); err == nil {
		t.Fatal("non-loopback endpoint must fail validation")
	}
}

func TestValidateBounds(t *testing.T) {
	c := Default()
	c.BreakAfter = 0
	if err := Validate(c); err == nil {
		t.Fatal("break_after < 1 must fail")
	}
	c = Default()
	c.Retries = -1
	if err := Validate(c); err == nil {
		t.Fatal("negative retries must fail")
	}
	c = Default()
	c.Harness = "bogus"
	if err := Validate(c); err == nil {
		t.Fatal("unknown harness must fail")
	}
}

func TestLoadOverlay(t *testing.T) {
	dir := t.TempDir()
	p := filepath.Join(dir, "cfg.json")
	body := `{"harness":"goose","target":"darwin-metal","budget":{"wall_clock":3600000000000}}`
	if err := os.WriteFile(p, []byte(body), 0o600); err != nil {
		t.Fatal(err)
	}
	c, err := Load(p)
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	if c.Harness != HarnessGoose {
		t.Errorf("harness = %q, want goose", c.Harness)
	}
	if c.Target != TargetDarwinMetal {
		t.Errorf("target = %q, want darwin-metal", c.Target)
	}
	if c.Budget.WallClock != time.Hour {
		t.Errorf("wall_clock = %s, want 1h", c.Budget.WallClock)
	}
	// Endpoint untouched -> still loopback default.
	if c.Endpoint != "http://127.0.0.1:8080" {
		t.Errorf("endpoint = %q, want default loopback", c.Endpoint)
	}
}

func TestLoadMissingReturnsDefaults(t *testing.T) {
	c, err := Load(filepath.Join(t.TempDir(), "nope.json"))
	if err != nil {
		t.Fatalf("missing file must return defaults, got %v", err)
	}
	if c.Harness != HarnessOpenCode {
		t.Errorf("harness = %q, want default opencode", c.Harness)
	}
}

// TestHarnessSelectionBuiltin models HARNESS-010: config selects the harness
// implementation; "builtin" (the serving-backed harness) is valid alongside the
// external harnesses, and an unknown value is rejected.
func TestHarnessSelectionBuiltin(t *testing.T) {
	for _, h := range []Harness{HarnessBuiltin, HarnessOpenCode, HarnessGoose} {
		c := Default()
		c.Harness = h
		if err := Validate(c); err != nil {
			t.Errorf("harness %q must be valid: %v", h, err)
		}
	}
	c := Default()
	c.Harness = "bogus"
	if err := Validate(c); err == nil {
		t.Error("unknown harness must be rejected")
	}
}

// TestTuningForModel covers the catalog-driven per-model tuning lookup (SERVE-020):
// the operator's Ollama tag is matched to a catalog entry by its `ollama` tag.
func TestTuningForModel(t *testing.T) {
	cat := []byte(`{"models":[
		{"id":"qwen3-coder-30b-a3b","ollama":"qwen3-coder","tuning":{"temperature":0.7,"num_ctx":16384}},
		{"id":"phi-4-mini","ollama":"phi4-mini"}
	]}`)
	if tn := TuningForModel("qwen3-coder:30b", cat); tn == nil || tn.Temperature == nil || *tn.Temperature != 0.7 {
		t.Errorf("qwen3-coder:30b should match the qwen3-coder tuning, got %+v", tn)
	}
	if tn := TuningForModel("phi4-mini:latest", cat); tn != nil {
		t.Error("phi4-mini carries no tuning -> nil")
	}
	if tn := TuningForModel("unknown:1b", cat); tn != nil {
		t.Error("unmatched model -> nil")
	}
	if tn := TuningForModel("", cat); tn != nil {
		t.Error("empty model -> nil")
	}
}

// TestTuningForGGUF covers the GGUF-keyed tuning lookup the production serving launch
// uses (SERVE-017): the calibration's model path is matched to the catalog by file.
func TestTuningForGGUF(t *testing.T) {
	cat := []byte(`{"models":[
		{"id":"qwen3-coder-30b-a3b","file":"Qwen3-Coder-30B-A3B-Instruct-UD-Q4_K_XL.gguf","tuning":{"num_ctx":16384}},
		{"id":"laguna","file":"laguna.gguf"}
	]}`)
	if tn := TuningForGGUF("/models/Qwen3-Coder-30B-A3B-Instruct-UD-Q4_K_XL.gguf", cat); tn == nil || tn.NumCtx == nil || *tn.NumCtx != 16384 {
		t.Errorf("GGUF path should match the catalog file -> num_ctx 16384, got %+v", tn)
	}
	if tn := TuningForGGUF("/models/laguna.gguf", cat); tn != nil {
		t.Error("laguna carries no tuning -> nil")
	}
	if tn := TuningForGGUF("/models/unknown.gguf", cat); tn != nil {
		t.Error("unmatched GGUF -> nil")
	}
}

// TestDefaultModelForTarget covers the target-aware default model (RUNQ-004): gemma is the
// CPU default (the proven CPU completer), qwen3-coder the darwin-metal default.
func TestDefaultModelForTarget(t *testing.T) {
	if m := DefaultModelForTarget(TargetLinuxCPU); m != "gemma4-qat:32k" {
		t.Errorf("linux-cpu default = %q, want gemma4-qat:32k (the CPU-capable completer)", m)
	}
	if m := DefaultModelForTarget(TargetDarwinMetal); m != "qwen3-coder:30b" {
		t.Errorf("darwin-metal default = %q, want qwen3-coder:30b", m)
	}
}
