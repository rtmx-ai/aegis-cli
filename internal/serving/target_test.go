package serving

import "testing"

// TestTarget verifies one calibration shape serves both targets through the
// same LaunchArgs entrypoint, differing only at launch per the target field.
func TestTargetOneConfigBothTargets(t *testing.T) {
	cases := []struct {
		name    string
		cal     *Calibration
		taskset bool
		ngl     string
	}{
		{"linux", &Calibration{Target: TargetLinuxCPU, Threads: 8, Batch: 256, NGL: 0, Model: "/m.gguf", Port: 8080}, true, "-ngl 0"},
		{"metal", &Calibration{Target: TargetDarwinMetal, Batch: 256, NGL: 999, Model: "/m.gguf", Port: 8080}, false, "-ngl 999"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			args, err := LaunchArgs(tc.cal)
			if err != nil {
				t.Fatalf("%s: %v", tc.name, err)
			}
			has := false
			for _, a := range args {
				if a == "taskset" {
					has = true
				}
			}
			if has != tc.taskset {
				t.Errorf("%s: taskset present=%v, want %v", tc.name, has, tc.taskset)
			}
		})
	}
}
