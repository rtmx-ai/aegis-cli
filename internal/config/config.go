// Package config loads and validates aegis-cli configuration.
//
// Every default is offline-safe: any setting that could cause network egress
// defaults to off/false, and the only network address the orchestrator may use
// is an explicit loopback endpoint. Validation rejects non-loopback endpoints
// and out-of-range bounds so a misconfigured run fails closed.
package config

import (
	"encoding/json"
	"fmt"
	"net"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"time"
)

// Harness identifies which coding-agent harness the loop drives.
type Harness string

// Supported harnesses.
const (
	// HarnessBuiltin is the in-binary serving-backed harness (no external
	// harness process; drives the local model over loopback directly).
	HarnessBuiltin  Harness = "builtin"
	HarnessOpenCode Harness = "opencode"
	HarnessGoose    Harness = "goose"
)

// Target identifies the serving build/launch target.
type Target string

// Supported serving targets.
const (
	TargetLinuxCPU    Target = "linux-cpu"
	TargetDarwinMetal Target = "darwin-metal"
)

// Budget bounds a single unattended run: a maximum number of requirements and a
// wall-clock ceiling. A zero value in either field means "unbounded" for that
// dimension, but the conservative defaults set both.
type Budget struct {
	// MaxRequirements caps how many requirements a session will attempt.
	MaxRequirements int `json:"max_requirements"`
	// WallClock caps total session duration.
	WallClock time.Duration `json:"wall_clock"`
	// PerTaskTokens caps the tokens spent working a single requirement before it is
	// parked (LONGRUN-008); 0 = unlimited. Distinct from the session-wide caps.
	PerTaskTokens int `json:"per_task_tokens"`
	// PerTaskWallClock caps the wall-clock spent working a single requirement before
	// it is parked (LONGRUN-008); 0 = unlimited.
	PerTaskWallClock time.Duration `json:"per_task_wall_clock"`
}

// Config is the fully-resolved orchestrator configuration.
//
// It is constructed with offline-safe defaults via Default, optionally
// overlaid from a file via Load, and must pass Validate before use.
type Config struct {
	// Endpoint is the local model endpoint. It MUST be loopback.
	Endpoint string `json:"endpoint"`
	// Harness selects the coding-agent adapter.
	Harness Harness `json:"harness"`
	// Target selects the serving build/launch target.
	Target Target `json:"target"`
	// Retries is the per-requirement verify retry count (N) before escalation.
	Retries int `json:"retries"`
	// BreakAfter trips the circuit breaker after M consecutive failures.
	BreakAfter int `json:"break_after"`
	// Budget bounds an unattended run.
	Budget Budget `json:"budget"`
	// AuditPath is the local, in-enclave append-only audit log file.
	AuditPath string `json:"audit_path"`
	// CalibrationPath is the serving calibration file (host-tuned).
	CalibrationPath string `json:"calibration_path"`
	// ModelID and ModelDigest, when set, are the expected served-model identity
	// checked at run start (SERVE-013). Empty values skip that part of the gate.
	ModelID     string `json:"model_id,omitempty"`
	ModelDigest string `json:"model_digest,omitempty"`
	// AllowEgress, when true, disables the loopback-only guard. It defaults to
	// false and exists only so the guard itself is testable; production runs
	// must leave it false.
	AllowEgress bool `json:"allow_egress"`
	// Tuning carries the per-model serving knobs (SERVE-020). When set, the launch
	// renders them so the model emits reliable tool calls. Populated from the model
	// catalog by ModelID; nil means "use the harness/serving defaults".
	Tuning *ModelTuning `json:"tuning,omitempty"`
	// MaxSteps bounds the agent's tool-call rounds per requirement (RUNQ-003), so a
	// capable-but-rambling model is stopped instead of looping. 0 -> DefaultMaxSteps at run.
	MaxSteps int `json:"max_steps,omitempty"`
	// MaxOutputTokens bounds per-turn generation (RUNQ-003) so runaway output is cut, not
	// allowed to run away (best-effort via the Ollama num_predict option). Set it high
	// enough that a normal turn is not truncated. 0 -> DefaultMaxOutputTokens at run.
	MaxOutputTokens int `json:"max_output_tokens,omitempty"`
	// Interactive marks an interactive TUI session (vs a headless run). It selects the proactive
	// persona over the tight headless directives (PERSONA-001). Launch-set, never persisted.
	Interactive bool `json:"-"`
}

// Run-policy limit defaults (RUNQ-003): bound a slow/rambling local model without
// truncating a normal coding turn. Applied by `aegis run` when the config leaves them 0.
const (
	DefaultMaxSteps        = 40
	DefaultMaxOutputTokens = 8192
)

// CPU/GPU default model ids (Ollama tags) used when a run names no model.
const (
	defaultModelLinuxCPU    = "gemma4-qat:32k"
	defaultModelDarwinMetal = "gemma4-qat:32k"
)

// DefaultModelForTarget returns the recommended local model id for a serving target when a
// run does not name one. On linux-cpu the CPU-capable completer (gemma4-qat) is the default:
// RUNQ-004 proved it closes real tasks on CPU, while the qwen3-coder bundle default
// fast-fails there (its Ollama tag emits Qwen-native XML tool calls that leak as text, and
// runs at Ollama's small default context). On darwin-metal the agentic primary
// (qwen3-coder) is the default. The bundle GGUF pin (deploy/models/MODEL_REF) is separate.
func DefaultModelForTarget(t Target) string {
	if t == TargetDarwinMetal {
		return defaultModelDarwinMetal
	}
	return defaultModelLinuxCPU
}

// ModelTuning is the per-model serving tuning the SERVE-016 bake-off characterization
// recommends (SERVE-020): sampling, context window, and thinking control. Nil fields
// are omitted from the rendered config. Sampling (temperature/top_p) is delivered
// reliably via the harness; the Ollama extensions (top_k/min_p/repeat_penalty/num_ctx/
// think) are forwarded best-effort — the robust path for num_ctx/think is the serving
// launch (llama.cpp --ctx-size, or an Ollama Modelfile). See docs/serve-016-bakeoff.md.
type ModelTuning struct {
	Temperature   *float64 `json:"temperature,omitempty"`
	TopP          *float64 `json:"top_p,omitempty"`
	TopK          *int     `json:"top_k,omitempty"`
	MinP          *float64 `json:"min_p,omitempty"`
	RepeatPenalty *float64 `json:"repeat_penalty,omitempty"`
	NumCtx        *int     `json:"num_ctx,omitempty"`
	Think         *bool    `json:"think,omitempty"`
}

// TuningForModel returns the recommended ModelTuning for the operator's modelID from a
// model-catalog JSON document (deploy/models/catalog.json), matched by each entry's
// `ollama` tag (exact, then prefix — e.g. tag "qwen3-coder" matches "qwen3-coder:30b").
// Returns nil when no catalog entry matches or carries tuning.
func TuningForModel(modelID string, catalogJSON []byte) *ModelTuning {
	if modelID == "" {
		return nil
	}
	var cat struct {
		Models []struct {
			Ollama string       `json:"ollama"`
			Tuning *ModelTuning `json:"tuning"`
		} `json:"models"`
	}
	if err := json.Unmarshal(catalogJSON, &cat); err != nil {
		return nil
	}
	for _, e := range cat.Models {
		if e.Tuning != nil && e.Ollama != "" && (e.Ollama == modelID || strings.HasPrefix(modelID, e.Ollama)) {
			return e.Tuning
		}
	}
	return nil
}

// TuningForGGUF returns the recommended ModelTuning for a GGUF model path from a
// catalog JSON document, matched by the catalog entry's `file` (basename). The
// production serving launch (SERVE-017) uses it to carry the per-model num_ctx onto
// llama-server --ctx-size. Returns nil when no entry matches or carries tuning.
func TuningForGGUF(ggufPath string, catalogJSON []byte) *ModelTuning {
	if ggufPath == "" {
		return nil
	}
	base := filepath.Base(ggufPath)
	var cat struct {
		Models []struct {
			File   string       `json:"file"`
			Tuning *ModelTuning `json:"tuning"`
		} `json:"models"`
	}
	if err := json.Unmarshal(catalogJSON, &cat); err != nil {
		return nil
	}
	for _, e := range cat.Models {
		if e.Tuning != nil && e.File != "" && e.File == base {
			return e.Tuning
		}
	}
	return nil
}

// Default returns a Config populated with offline-safe defaults.
func Default() Config {
	return Config{
		Endpoint:        "http://127.0.0.1:8080",
		Harness:         HarnessOpenCode,
		Target:          TargetLinuxCPU,
		Retries:         2,
		BreakAfter:      3,
		Budget:          Budget{MaxRequirements: 40, WallClock: 8 * time.Hour},
		AuditPath:       "aegis-audit.log",
		CalibrationPath: "deploy/llama-server/calibration.json",
		AllowEgress:     false,
	}
}

// Load reads a JSON config file and overlays it onto the offline-safe defaults.
// A missing path returns the defaults unchanged. The result is validated.
func Load(path string) (Config, error) {
	cfg := Default()
	if path == "" {
		return cfg, Validate(cfg)
	}
	data, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			return cfg, Validate(cfg)
		}
		return cfg, fmt.Errorf("config: read %s: %w", path, err)
	}
	if err := json.Unmarshal(data, &cfg); err != nil {
		return cfg, fmt.Errorf("config: parse %s: %w", path, err)
	}
	return cfg, Validate(cfg)
}

// Validate checks a Config for offline-safety and sane bounds. It is the single
// gate every config passes before the loop runs.
func Validate(c Config) error {
	u, err := url.Parse(c.Endpoint)
	if err != nil {
		return fmt.Errorf("config: invalid endpoint %q: %w", c.Endpoint, err)
	}
	if u.Scheme != "http" && u.Scheme != "https" {
		return fmt.Errorf("config: endpoint scheme must be http(s): %q", c.Endpoint)
	}
	if !c.AllowEgress && !isLoopbackHost(u.Hostname()) {
		return fmt.Errorf("config: endpoint %q is not loopback (egress is build-failing)", c.Endpoint)
	}
	switch c.Harness {
	case HarnessBuiltin, HarnessOpenCode, HarnessGoose:
	default:
		return fmt.Errorf("config: unknown harness %q", c.Harness)
	}
	switch c.Target {
	case TargetLinuxCPU, TargetDarwinMetal:
	default:
		return fmt.Errorf("config: unknown target %q", c.Target)
	}
	if c.Retries < 0 {
		return fmt.Errorf("config: retries must be >= 0, got %d", c.Retries)
	}
	if c.BreakAfter < 1 {
		return fmt.Errorf("config: break_after must be >= 1, got %d", c.BreakAfter)
	}
	if c.Budget.MaxRequirements < 0 {
		return fmt.Errorf("config: budget max_requirements must be >= 0, got %d", c.Budget.MaxRequirements)
	}
	if c.Budget.WallClock < 0 {
		return fmt.Errorf("config: budget wall_clock must be >= 0, got %s", c.Budget.WallClock)
	}
	if c.AuditPath == "" {
		return fmt.Errorf("config: audit_path must be set")
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
