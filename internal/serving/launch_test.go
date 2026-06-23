package serving

import (
	"strings"
	"testing"
)

func TestLaunchArgsUncalibratedIsHardError(t *testing.T) {
	if _, err := LaunchArgs(nil); err == nil {
		t.Fatal("uncalibrated launch must be a hard error")
	}
}

func TestLaunchArgsLinuxCPU(t *testing.T) {
	cal := &Calibration{Target: TargetLinuxCPU, Threads: 16, Batch: 512, NGL: 0, Model: "/m.gguf", Port: 8080}
	args, err := LaunchArgs(cal)
	if err != nil {
		t.Fatalf("launch args: %v", err)
	}
	joined := strings.Join(args, " ")
	if !strings.Contains(joined, "taskset") {
		t.Error("linux-cpu must pin with taskset")
	}
	if !strings.Contains(joined, "nice") {
		t.Error("linux-cpu must de-prioritize with nice")
	}
	if !strings.Contains(joined, "-ngl 0") {
		t.Error("linux-cpu must run CPU-only (-ngl 0)")
	}
	if !strings.Contains(joined, "127.0.0.1") {
		t.Error("must bind loopback")
	}
}

func TestLaunchArgsDarwinMetal(t *testing.T) {
	cal := &Calibration{Target: TargetDarwinMetal, Batch: 512, NGL: 999, Model: "/m.gguf", Port: 8080}
	args, err := LaunchArgs(cal)
	if err != nil {
		t.Fatalf("launch args: %v", err)
	}
	joined := strings.Join(args, " ")
	if strings.Contains(joined, "taskset") {
		t.Error("darwin-metal must NOT use taskset")
	}
	if !strings.Contains(joined, "-ngl 999") {
		t.Error("darwin-metal must offload all layers (-ngl 999)")
	}
	if !strings.Contains(joined, "nice") {
		t.Error("darwin-metal still applies nice")
	}
}
