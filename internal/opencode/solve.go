package opencode

import (
	"context"
	"fmt"
	"net"

	"github.com/rtmx-ai/aegis-cli/internal/config"
)

// SolveOptions configures one headless agent run.
type SolveOptions struct {
	Workdir  string // project directory the agent works in
	Prompt   string // the task prompt
	Model    string // model id (defaults to cfg.ModelID)
	Port     int    // retained for the serve-API path; 0 picks a free loopback port
	NoIntent bool   // omit the rtmx MCP intent layer (intent-bench "control", BENCH-004)
}

// SolveResult is the outcome of a headless run.
type SolveResult struct {
	SessionID string
	Messages  []TranscriptMessage
	// TimedOut is set when the run hit its wall-clock budget and was aborted;
	// Messages then holds the partial transcript (RUNQ-001).
	TimedOut bool
}

// Solve runs OpenCode headlessly for one prompt and returns the transcript + usage.
// It drives OpenCode through its `serve` HTTP API (BENCH-006/008) — the working
// headless surface — not the classic `opencode run`, which wedges offline. We drive
// OpenCode; we do not reimplement it.
func Solve(ctx context.Context, cfg config.Config, explicitBin string, opts SolveOptions) (*SolveResult, error) {
	bin, err := ResolveBinary(explicitBin)
	if err != nil {
		return nil, err
	}
	return solveDrive(ctx, bin, cfg, opts)
}

// solveDrive is the seam between Solve and the serve drive, overridable in tests so
// Solve's resolve+route behavior is unit-testable without a real OpenCode binary.
var solveDrive = realSolveDrive

// realSolveDrive launches `opencode serve` (loopback) rooted at the workdir and
// drives one synchronous turn through it, honoring the caller's wall-clock budget
// (RUNQ-001): the run's context bounds StartServe + Drive, and on expiry the serve
// process group is torn down and the partial transcript returned (TimedOut set).
func realSolveDrive(ctx context.Context, bin string, cfg config.Config, opts SolveOptions) (*SolveResult, error) {
	port := opts.Port
	if port == 0 {
		p, err := freeLoopbackPort()
		if err != nil {
			return nil, err
		}
		port = p
	}
	client, stop, err := StartServe(ctx, bin, cfg, opts.Workdir, port, !opts.NoIntent)
	if err != nil {
		return nil, err
	}
	defer stop()
	model := opts.Model
	if model == "" {
		model = cfg.ModelID
	}
	return client.Drive(ctx, Model{ProviderID: "local", ModelID: model}, opts.Prompt)
}

// freeLoopbackPort returns an unused loopback TCP port for the serve API.
func freeLoopbackPort() (int, error) {
	l, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		return 0, fmt.Errorf("opencode serve: pick port: %w", err)
	}
	defer l.Close()
	return l.Addr().(*net.TCPAddr).Port, nil
}
