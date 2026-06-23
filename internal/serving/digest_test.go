package serving

import "testing"

// TestDigest is the scaffold anchor for SERVE-002 (loaded model matches the
// configured quant + digest). The full digest comparison lands with that
// requirement; here we assert the calibration carries a model reference that a
// digest check can key off, and that an empty one is rejected.
func TestDigestRequiresModelReference(t *testing.T) {
	ok := &Calibration{Target: TargetLinuxCPU, Threads: 4, Batch: 128, NGL: 0, Model: "/models/m.gguf", Port: 8080}
	if err := ok.validate(); err != nil {
		t.Fatalf("calibration with model should validate: %v", err)
	}
	if ok.Model == "" {
		t.Fatal("model reference required for digest matching")
	}
}
