package opencode

import (
	"context"

	"github.com/rtmx-ai/aegis-cli/internal/config"
)

// SolveOptions configures one headless agent run.
type SolveOptions struct {
	Workdir string // project directory the agent works in
	Prompt  string // the task prompt
	Model   string // model id (defaults to cfg.ModelID)
	Port    int    // retained for the serve-API path; unused by the `opencode run` engine
}

// SolveResult is the outcome of a headless run.
type SolveResult struct {
	SessionID string
	Messages  []TranscriptMessage
	// TimedOut is set when the run hit its wall-clock budget and was aborted;
	// Messages then holds the partial transcript (RUNQ-001).
	TimedOut bool
}

// Solve runs OpenCode headlessly for one prompt and returns the transcript +
// usage. It drives the classic `opencode run` command (BENCH-001) — the working
// headless surface; the serve-API client (serve.go) is retained for when upstream
// lands the v2 HTTP run. We drive OpenCode; we do not reimplement it.
func Solve(ctx context.Context, cfg config.Config, explicitBin string, opts SolveOptions) (*SolveResult, error) {
	bin, err := ResolveBinary(explicitBin)
	if err != nil {
		return nil, err
	}
	return RunHeadless(ctx, bin, cfg, opts.Workdir, opts.Model, opts.Prompt)
}
