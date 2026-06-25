package opencode

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"strings"
	"time"

	"github.com/rtmx-ai/aegis-cli/internal/config"
)

// RunHeadless executes the classic `opencode run` headless command (BENCH-001):
// one prompt, non-interactive, streams JSON events, exits when the session goes
// idle. It runs in workdir against the operator's local model, under the air-gap
// env + the inline hardened config, with --pure (no external plugins). All
// traffic is loopback. This is the engine behind `aegis run <prompt>`.
func RunHeadless(ctx context.Context, bin string, cfg config.Config, workdir, model, prompt string) (*SolveResult, error) {
	if model == "" {
		model = cfg.ModelID
	}
	args := []string{"run", "--pure", "--format", "json", "--model", "local/" + model, "--dir", workdir, prompt}
	cmd := exec.CommandContext(ctx, bin, args...)
	cmd.Env = append(os.Environ(), airgapEnv(cfg)...)
	// RUNQ-001: on budget expiry, force the run down promptly even if a child holds
	// the output pipe open — bound the post-cancel wait, then kill + close pipes.
	cmd.WaitDelay = 3 * time.Second
	setProcGroup(cmd)
	var out bytes.Buffer
	cmd.Stdout = &out
	cmd.Stderr = os.Stderr
	err := cmd.Run()
	msgs := parseRunEvents(out.String()) // parse whatever was captured (partial on kill)
	if err != nil {
		// RUNQ-001: a deadline/cancel is not a hard error — the child is killed and
		// we return the partial transcript so the caller can record it.
		if ctx.Err() != nil {
			return &SolveResult{Messages: msgs, TimedOut: ctx.Err() == context.DeadlineExceeded}, nil
		}
		return nil, fmt.Errorf("opencode run: %w", err)
	}
	return &SolveResult{Messages: msgs}, nil
}

// parseRunEvents flattens `opencode run --format json` NDJSON events into a single
// assistant transcript message: concatenated text + the final usage/finish. The
// event stream uses text parts and step_finish records carrying tokens.
func parseRunEvents(s string) []TranscriptMessage {
	msg := TranscriptMessage{Role: "assistant"}
	var text strings.Builder
	sc := bufio.NewScanner(strings.NewReader(s))
	sc.Buffer(make([]byte, 1<<20), 16<<20)
	for sc.Scan() {
		line := strings.TrimSpace(sc.Text())
		if line == "" || line[0] != '{' {
			continue
		}
		var e struct {
			Type string `json:"type"`
			Part struct {
				Text   string `json:"text"`
				Reason string `json:"reason"`
				Tokens Tokens `json:"tokens"`
			} `json:"part"`
		}
		if json.Unmarshal([]byte(line), &e) != nil {
			continue
		}
		if e.Part.Text != "" {
			text.WriteString(e.Part.Text)
		}
		if e.Type == "step_finish" {
			msg.Finish = e.Part.Reason
			if e.Part.Tokens.Total > 0 || e.Part.Tokens.Output > 0 {
				msg.Tokens = e.Part.Tokens
			}
		}
	}
	msg.Text = text.String()
	if msg.Text == "" && msg.Finish == "" {
		return nil
	}
	return []TranscriptMessage{msg}
}
