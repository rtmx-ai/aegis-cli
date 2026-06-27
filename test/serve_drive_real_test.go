package offline

import (
	"context"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
	"time"

	"github.com/rtmx-ai/aegis-cli/internal/config"
	"github.com/rtmx-ai/aegis-cli/internal/opencode"
)

// TestServeDriveRealBinary → REQ-BENCH-007: validate the serve DRIVE against the
// REAL self-built OpenCode binary (not a mock). It starts `opencode serve`, drives
// a trivial edit task on a live local model, and asserts a real transcript with
// per-message token usage — exercising the real routes + response shapes that a
// mock can silently get wrong. (It caught two such bugs: the flat session-create
// and message-list shapes.) Whether the model actually lands the edit and the task
// tests pass is model capability — REQ-RUNQ-004, gated on the SERVE-016 bake-off —
// so that outcome is logged here, not asserted.
//
// Gated/manual — skipped in CI. To run it, stand up the stack and set:
//
//	AEGIS_REAL_ENDPOINT=http://127.0.0.1:11434   # loopback model endpoint
//	AEGIS_REAL_MODEL=<served-model-id>
//
// plus a resolvable OpenCode binary and ripgrep (bundled via OC-009 /
// scripts/stage-ripgrep.sh, or on PATH). The plugin seed (OC-010) is materialized
// automatically by the launch.
func TestServeDriveRealBinary(t *testing.T) {
	endpoint := os.Getenv("AEGIS_REAL_ENDPOINT")
	model := os.Getenv("AEGIS_REAL_MODEL")
	if endpoint == "" || model == "" {
		t.Skip("set AEGIS_REAL_ENDPOINT + AEGIS_REAL_MODEL to run the real-binary serve drive")
	}
	bin, err := opencode.ResolveBinary("")
	if err != nil {
		t.Skipf("opencode binary not resolvable (bundle it or put it on PATH): %v", err)
	}
	// OpenCode fetches ripgrep from github at bootstrap unless it can resolve `rg`
	// (OC-009). Require a staged or on-PATH rg so the air-gapped run does not hang.
	if _, ok := opencode.ResolveRipgrep(); !ok {
		if _, err := exec.LookPath("rg"); err != nil {
			t.Skip("no ripgrep resolvable (run scripts/stage-ripgrep.sh or put rg on PATH)")
		}
	}

	// A trivial, dependency-free Go edit task with a failing test.
	ws := t.TempDir()
	writeFile(t, ws, "go.mod", "module task\n\ngo 1.21\n")
	writeFile(t, ws, "add.go", "package task\n\nfunc Add(a, b int) int { return 0 }\n")
	writeFile(t, ws, "add_test.go", "package task\n\nimport \"testing\"\n\nfunc TestAdd(t *testing.T) {\n\tif Add(2, 3) != 5 {\n\t\tt.Fatalf(\"Add(2,3)=%d, want 5\", Add(2, 3))\n\t}\n}\n")
	if taskTestPasses(ws) {
		t.Fatal("precondition: the task test must fail before the agent edits it")
	}

	cfg := config.Default()
	cfg.Endpoint = endpoint
	cfg.ModelID = model

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Minute)
	defer cancel()

	client, stop, err := opencode.StartServe(ctx, bin, cfg, ws, freePort(t), true)
	if err != nil {
		t.Fatalf("StartServe (real opencode): %v", err)
	}
	defer stop()

	res, err := client.Drive(ctx, opencode.Model{ProviderID: "local", ModelID: model},
		"Edit add.go so that Add(a, b) returns a + b instead of 0. Use the edit tool to modify add.go.")
	if err != nil {
		t.Fatalf("Drive against real opencode + model: %v", err)
	}

	// The drive must return a real transcript: at least the user + one assistant
	// message, with per-message token usage surfaced (this is what the corrected
	// routes/shapes deliver, and what a wrong shape would empty out).
	if len(res.Messages) < 2 {
		t.Fatalf("expected a multi-message transcript from the real drive; got %d: %+v", len(res.Messages), res.Messages)
	}
	var assistantTokens float64
	sawAssistant := false
	for _, m := range res.Messages {
		if m.Role == "assistant" {
			sawAssistant = true
			assistantTokens += m.Tokens.Total
		}
	}
	if !sawAssistant {
		t.Errorf("no assistant message in transcript: %+v", res.Messages)
	}
	if assistantTokens == 0 {
		t.Errorf("expected non-zero assistant token usage; got transcript %+v", res.Messages)
	}
	t.Logf("real-binary serve drive OK: %d messages, assistant tokens=%v", len(res.Messages), assistantTokens)

	// Whether the model actually landed the edit + made the task tests pass is model
	// capability (REQ-RUNQ-004, gated on the SERVE-016 bake-off): log it, don't gate.
	if taskTestPasses(ws) {
		t.Log("bonus: the model completed the edit and the task tests PASS")
	} else {
		t.Log("note: the model did not land the edit (RUNQ-002/RUNQ-004 — model capability, not the drive)")
	}
}

func writeFile(t *testing.T, dir, name, content string) {
	t.Helper()
	if err := os.WriteFile(filepath.Join(dir, name), []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
}

// taskTestPasses runs `go test` in the task workdir (a standalone module with no
// deps, so it needs no network). GOFLAGS is reset so an outer -mod=vendor does not
// leak into the dependency-free module.
func taskTestPasses(dir string) bool {
	cmd := exec.Command("go", "test", "./...")
	cmd.Dir = dir
	cmd.Env = append(os.Environ(), "GOFLAGS=-mod=mod")
	return cmd.Run() == nil
}

func freePort(t *testing.T) int {
	t.Helper()
	l, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("freePort: %v", err)
	}
	defer l.Close()
	return l.Addr().(*net.TCPAddr).Port
}
