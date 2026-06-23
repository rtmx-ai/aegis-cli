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
