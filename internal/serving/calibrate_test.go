package serving

import (
	"os"
	"path/filepath"
	"testing"
)

func writeCalibration(t *testing.T, body string) string {
	t.Helper()
	p := filepath.Join(t.TempDir(), "calibration.json")
	if err := os.WriteFile(p, []byte(body), 0o600); err != nil {
		t.Fatal(err)
	}
	return p
}

func TestLoadCalibrationLinuxCPU(t *testing.T) {
	p := writeCalibration(t, `{"target":"linux-cpu","threads":16,"batch":512,"ngl":0,"model":"/m.gguf","port":8080}`)
	c, err := LoadCalibration(p)
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	if c.Target != TargetLinuxCPU || c.Threads != 16 {
		t.Errorf("unexpected calibration: %+v", c)
	}
}

func TestLoadCalibrationRejectsBadLinuxNGL(t *testing.T) {
	p := writeCalibration(t, `{"target":"linux-cpu","threads":16,"batch":512,"ngl":999,"model":"/m.gguf","port":8080}`)
	if _, err := LoadCalibration(p); err == nil {
		t.Fatal("linux-cpu with ngl!=0 must fail validation")
	}
}

func TestLoadCalibrationMissingFile(t *testing.T) {
	if _, err := LoadCalibration(filepath.Join(t.TempDir(), "nope.json")); err == nil {
		t.Fatal("missing calibration file must error")
	}
}
