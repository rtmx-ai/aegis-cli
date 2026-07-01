// Package serving owns the local inference endpoint: calibration loading,
// per-target launch-arg construction, resource policy, and a loopback health
// probe.
//
// The only network call anywhere in aegis-cli is the health probe here, and it
// is restricted to a loopback endpoint. A launch with no calibration loaded is
// a hard error: the system calibrates, it does not guess.
package serving

import (
	"context"
	"encoding/json"
	"fmt"
	"net"
	"net/http"
	"net/url"
	"os"
	"strconv"
	"time"
)

// Target identifies the serving build/launch target. It mirrors
// config.Target but is duplicated here to keep serving free of a config import.
type Target string

// Supported serving targets.
const (
	TargetLinuxCPU    Target = "linux-cpu"
	TargetDarwinMetal Target = "darwin-metal"
)

// Calibration is a host-tuned serving configuration loaded from
// deploy/llama-server/calibration.json.
type Calibration struct {
	// Target is the host target this calibration was tuned for.
	Target Target `json:"target"`
	// Threads is the inference thread count (linux-cpu knob).
	Threads int `json:"threads"`
	// Batch is the batch size.
	Batch int `json:"batch"`
	// NGL is the number of layers to offload to the GPU. On linux-cpu this is
	// 0; on darwin-metal it is all layers (e.g. 999).
	NGL int `json:"ngl"`
	// Model is the path to the GGUF model file.
	Model string `json:"model"`
	// Port is the loopback port the server binds.
	Port int `json:"port"`
	// CtxSize is the context window served (llama-server --ctx-size). It carries the
	// selected model's num_ctx (SERVE-020 tuning) onto the production path so it is a
	// REAL serving knob, not the small default llama.cpp/Ollama fall back to (which
	// silently truncates the harness's front-loaded tool definitions). 0 ->
	// DefaultCtxSize at launch.
	CtxSize int `json:"ctx_size,omitempty"`
	// Reasoning is the calibrated reasoning budget (THINK-001): whether the model
	// reasons on hard tasks and a token cap. Zero value = reasoning off, the
	// small-model default (long CoT is a latency tax and can lower accuracy <~10B).
	Reasoning Reasoning `json:"reasoning,omitempty"`
}

// DefaultCtxSize is the context window the production launch uses when the calibration sets none.
// Raised to 32768 (PERF-003): agentic harnesses front-load large tool definitions, and since the cold
// prefill is amortized by KV cache reuse (verified: 68ms cached vs 5727ms cold), a larger window is
// cheap after the one-time warm — 16k was the observed cause of "context size exceeded" on real tasks.
const DefaultCtxSize = 32768

// CtxSizeOrDefault returns the context window to serve: AEGIS_CTX_SIZE if the operator set it (the
// PERF-003 tunable), else the calibrated CtxSize, else DefaultCtxSize. The production launch never
// serves a small default context.
func (c *Calibration) CtxSizeOrDefault() int {
	if v := os.Getenv("AEGIS_CTX_SIZE"); v != "" {
		if n, err := strconv.Atoi(v); err == nil && n >= 512 {
			return n
		}
	}
	if c.CtxSize >= 512 {
		return c.CtxSize
	}
	return DefaultCtxSize
}

// LoadCalibration reads and validates a calibration file.
func LoadCalibration(path string) (*Calibration, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("serving: read calibration %s: %w", path, err)
	}
	var c Calibration
	if err := json.Unmarshal(data, &c); err != nil {
		return nil, fmt.Errorf("serving: parse calibration %s: %w", path, err)
	}
	if err := c.validate(); err != nil {
		return nil, err
	}
	return &c, nil
}

// validate checks a calibration for internal consistency per target.
func (c *Calibration) validate() error {
	switch c.Target {
	case TargetLinuxCPU:
		if c.Threads < 1 {
			return fmt.Errorf("serving: linux-cpu calibration needs threads >= 1")
		}
		if c.NGL != 0 {
			return fmt.Errorf("serving: linux-cpu calibration must set ngl=0, got %d", c.NGL)
		}
	case TargetDarwinMetal:
		if c.NGL <= 0 {
			return fmt.Errorf("serving: darwin-metal calibration must offload all layers (ngl>0), got %d", c.NGL)
		}
	default:
		return fmt.Errorf("serving: unknown calibration target %q", c.Target)
	}
	if c.Model == "" {
		return fmt.Errorf("serving: calibration missing model path")
	}
	return nil
}

// LaunchArgs builds the llama-server launch command for cal. A nil calibration
// is a hard error: uncalibrated launch is forbidden.
//
// On linux-cpu the server is pinned with taskset and de-prioritized with nice,
// runs CPU-only (-ngl 0), and uses the calibrated thread count. On
// darwin-metal there is no taskset, all layers offload (-ngl 999), and nice
// still applies.
func LaunchArgs(cal *Calibration) ([]string, error) {
	if cal == nil {
		return nil, fmt.Errorf("serving: uncalibrated launch is forbidden (no calibration loaded)")
	}
	if err := cal.validate(); err != nil {
		return nil, err
	}
	var args []string
	switch cal.Target {
	case TargetLinuxCPU:
		args = append(args,
			"taskset", "-c", fmt.Sprintf("0-%d", cal.Threads-1),
			"nice", "-n", "5",
			"llama-server",
			"--model", cal.Model,
			// --jinja uses the model's embedded chat template so native tool-call formats
			// (e.g. Qwen3-Coder's XML tags) are parsed into structured tool calls instead of
			// leaking into text — mandatory for correct agentic tool use (SERVE-017/022).
			"--jinja",
			"--threads", fmt.Sprintf("%d", cal.Threads),
			"--batch-size", fmt.Sprintf("%d", cal.Batch),
			"--ctx-size", fmt.Sprintf("%d", cal.CtxSizeOrDefault()),
			"-ngl", "0",
			"--host", "127.0.0.1",
			"--port", fmt.Sprintf("%d", cal.Port),
		)
	case TargetDarwinMetal:
		args = append(args,
			"nice", "-n", "5",
			"llama-server",
			"--model", cal.Model,
			"--jinja", // parse native tool-call templates (see linux-cpu branch, SERVE-017/022)
			"--batch-size", fmt.Sprintf("%d", cal.Batch),
			"--ctx-size", fmt.Sprintf("%d", cal.CtxSizeOrDefault()),
			"-ngl", "999",
			"--host", "127.0.0.1",
			"--port", fmt.Sprintf("%d", cal.Port),
		)
	default:
		return nil, fmt.Errorf("serving: unknown target %q", cal.Target)
	}
	return args, nil
}

// Endpoint describes the loopback model endpoint to probe.
type Endpoint struct {
	// URL is the base endpoint URL; it must be loopback.
	URL string
	// Client is the HTTP client used for the probe; nil uses a default with a
	// short timeout.
	Client *http.Client
}

// Health probes the endpoint's /health route over loopback and returns an
// error if it is unreachable, non-loopback, or returns non-200. It honors
// ctx for cancellation and timeout (e.g. the 2s probe budget).
func Health(ctx context.Context, ep Endpoint) error {
	u, err := url.Parse(ep.URL)
	if err != nil {
		return fmt.Errorf("serving: invalid endpoint %q: %w", ep.URL, err)
	}
	if !isLoopbackHost(u.Hostname()) {
		return fmt.Errorf("serving: endpoint %q is not loopback (egress forbidden)", ep.URL)
	}
	client := ep.Client
	if client == nil {
		client = &http.Client{Timeout: 2 * time.Second}
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, u.String()+"/health", nil)
	if err != nil {
		return fmt.Errorf("serving: build health request: %w", err)
	}
	resp, err := client.Do(req)
	if err != nil {
		return fmt.Errorf("serving: health probe failed: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("serving: health probe returned %d", resp.StatusCode)
	}
	return nil
}

// isLoopbackHost reports whether host is a loopback name or address.
func isLoopbackHost(host string) bool {
	if host == "localhost" {
		return true
	}
	if ip := net.ParseIP(host); ip != nil {
		return ip.IsLoopback()
	}
	return false
}
