package opencode

import (
	"context"
	"time"

	"github.com/rtmx-ai/aegis-cli/internal/config"
)

// SolveOptions configures one headless agent run.
type SolveOptions struct {
	Workdir string // project directory the agent works in
	Prompt  string // the task prompt
	Model   string // model id (defaults to cfg.ModelID)
	Port    int    // serve port (defaults to 8099)
}

// SolveResult is the outcome of a headless run.
type SolveResult struct {
	SessionID string
	Messages  []TranscriptMessage
}

// Solve runs OpenCode headlessly (BENCH-001): it starts `opencode serve` rooted
// at the workdir under the operator's hardened config, opens a session, posts the
// prompt for one autonomous agent turn against the local model, and returns the
// transcript + usage. All traffic is loopback. This is the engine behind
// `aegis solve` and intent-bench profiling.
func Solve(ctx context.Context, cfg config.Config, explicitBin string, opts SolveOptions) (*SolveResult, error) {
	bin, err := ResolveBinary(explicitBin)
	if err != nil {
		return nil, err
	}
	port := opts.Port
	if port == 0 {
		port = 8099
	}
	c, stop, err := StartServe(ctx, bin, cfg, opts.Workdir, port)
	if err != nil {
		return nil, err
	}
	defer stop()

	model := opts.Model
	if model == "" {
		model = cfg.ModelID
	}
	id, err := c.CreateSession(ctx, Model{ProviderID: "local", ModelID: model})
	if err != nil {
		return nil, err
	}
	if err := c.Prompt(ctx, id, opts.Prompt); err != nil {
		return nil, err
	}
	// Poll the transcript until the assistant turn finishes (the /wait endpoint is
	// a stub in the pinned preview). Returns whatever messages exist at timeout.
	deadline := time.Now().Add(10 * time.Minute)
	var msgs []TranscriptMessage
	for time.Now().Before(deadline) {
		msgs, err = c.Messages(ctx, id)
		if err != nil {
			return nil, err
		}
		if n := len(msgs); n > 0 && msgs[n-1].Role == "assistant" && msgs[n-1].Finish != "" {
			break
		}
		time.Sleep(2 * time.Second)
	}
	return &SolveResult{SessionID: id, Messages: msgs}, nil
}
