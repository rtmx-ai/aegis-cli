package serving

import (
	"context"
	"strconv"
	"strings"
	"testing"

	"github.com/rtmx-ai/aegis-cli/internal/mockmodel"
)

func TestLaunchArgsUncalibratedIsHardError(t *testing.T) {
	if _, err := LaunchArgs(nil); err == nil {
		t.Fatal("uncalibrated launch must be a hard error")
	}
}

// TestLlamaServerProduction → REQ-SERVE-017: the production llama.cpp serving path
// launches the selected GGUF under calibrated args, OpenAI-compatible, with a robust
// (never-default) context window so the harness's front-loaded tool definitions are
// not truncated — at parity with the Ollama spike. The launch command is asserted
// always; OpenAI-compatible parity is proven against a stub server; a real
// llama-server completion is the remaining gated check (needs a built binary + GGUF).
func TestLlamaServerProduction(t *testing.T) {
	// The selected model's num_ctx (SERVE-020) is carried onto --ctx-size.
	cal := &Calibration{Target: TargetLinuxCPU, Threads: 8, Batch: 256, NGL: 0, Model: "/models/m.gguf", Port: 8080, CtxSize: 16384}
	args, err := LaunchArgs(cal)
	if err != nil {
		t.Fatalf("launch args: %v", err)
	}
	joined := strings.Join(args, " ")
	for _, want := range []string{"llama-server", "--model /models/m.gguf", "--host 127.0.0.1", "--port 8080", "--ctx-size 16384"} {
		if !strings.Contains(joined, want) {
			t.Errorf("production launch must contain %q\n  got: %s", want, joined)
		}
	}

	// Robustness: a calibration with NO ctx_size still serves >= the agentic floor,
	// never llama.cpp's small default (the tool-definition truncation cause).
	if DefaultCtxSize < 16384 {
		t.Errorf("DefaultCtxSize must be >= 16384 (agentic harness floor), got %d", DefaultCtxSize)
	}
	bare := &Calibration{Target: TargetLinuxCPU, Threads: 8, Batch: 256, NGL: 0, Model: "/m.gguf", Port: 9090}
	if got := strings.Join(mustArgs(t, bare), " "); !strings.Contains(got, "--ctx-size "+strconv.Itoa(DefaultCtxSize)) {
		t.Errorf("uncalibrated ctx_size must fall back to DefaultCtxSize (%d)\n  got: %s", DefaultCtxSize, got)
	}

	// OpenAI-compatible parity: the same serving client that drives Ollama speaks
	// /v1/chat to llama-server. Prove PreflightSmoke + a completion succeed against a
	// stub OpenAI-compatible server (a real llama-server serves the identical surface).
	srv := mockmodel.New(mockmodel.Options{Responses: []mockmodel.Response{{Content: "pong"}}})
	defer srv.Close()
	c, err := NewClient(srv.URL())
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	if err := c.PreflightSmoke(context.Background(), "local"); err != nil {
		t.Fatalf("preflight smoke against OpenAI-compatible server must pass (parity): %v", err)
	}
}

func mustArgs(t *testing.T, cal *Calibration) []string {
	t.Helper()
	args, err := LaunchArgs(cal)
	if err != nil {
		t.Fatalf("launch args: %v", err)
	}
	return args
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
