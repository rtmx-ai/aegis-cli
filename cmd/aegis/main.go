// Command aegis is the aegis-cli orchestrator entrypoint.
//
// It is a thin dispatcher over the internal packages: subcommands are run,
// status, verify-env, propose and version. Real logic lives in internal/.
package main

import (
	"context"
	"encoding/csv"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"

	"github.com/rtmx-ai/aegis-cli/internal/audit"
	"github.com/rtmx-ai/aegis-cli/internal/bench"
	"github.com/rtmx-ai/aegis-cli/internal/config"
	"github.com/rtmx-ai/aegis-cli/internal/framing"
	"github.com/rtmx-ai/aegis-cli/internal/harness"
	"github.com/rtmx-ai/aegis-cli/internal/harness/goose"
	ocharness "github.com/rtmx-ai/aegis-cli/internal/harness/opencode"
	servingharness "github.com/rtmx-ai/aegis-cli/internal/harness/serving"
	"github.com/rtmx-ai/aegis-cli/internal/install"
	"github.com/rtmx-ai/aegis-cli/internal/loop"
	"github.com/rtmx-ai/aegis-cli/internal/metrics"
	"github.com/rtmx-ai/aegis-cli/internal/opencode"
	"github.com/rtmx-ai/aegis-cli/internal/origin"
	"github.com/rtmx-ai/aegis-cli/internal/rtmx"
	"github.com/rtmx-ai/aegis-cli/internal/serving"
)

// selectHarness constructs the harness adapter the config selects (HARNESS-010).
// The built-in serving-backed harness needs no external process; opencode/goose
// remain selectable behind the same seam.
func selectHarness(cfg config.Config) (harness.Adapter, error) {
	switch cfg.Harness {
	case config.HarnessBuiltin:
		return servingharness.New(cfg.Endpoint)
	case config.HarnessOpenCode:
		return ocharness.New(""), nil
	case config.HarnessGoose:
		return goose.New(""), nil
	default:
		return nil, fmt.Errorf("unknown harness %q", cfg.Harness)
	}
}

// version and commit are stamped at release build time via -ldflags.
var (
	version = "dev"
	commit  = "unknown"
)

// tuiLaunch is the TUI launch seam (overridable in tests). errNoModel marks the "no model
// provisioned" state (OC-025) as a NON-fatal signal — the TUI launches anyway so the operator can
// provision in-app, rather than exiting to the shell.
var (
	tuiLaunch       = opencode.Launch
	resolveOpencode = opencode.ResolveBinary
	errNoModel      = errors.New("no local model is provisioned")
)

// noModelErr wraps the provisioning guidance behind the errNoModel sentinel.
func noModelErr() error { return fmt.Errorf("%w\n%s", errNoModel, provisionGuidance()) }

func main() {
	os.Exit(run(os.Args[1:], os.Stdout, os.Stderr))
}

// run dispatches a subcommand and returns a process exit code. It is separated
// from main so tests can exercise it directly.
func run(args []string, stdout, stderr io.Writer) int {
	if len(args) == 0 {
		// The centerpiece: bare `aegis` launches the bundled OpenCode TUI.
		return cmdTUI(stdout, stderr)
	}
	cmd, rest := args[0], args[1:]
	switch cmd {
	case "tui":
		return cmdTUI(stdout, stderr)
	case "run", "solve": // solve: back-compat alias for the one-shot run
		return cmdRun(rest, stdout, stderr)
	case "loop":
		return cmdLoop(rest, stdout, stderr)
	case "rtmx", "code", "model": // hardened pass-through to an inner tool (SURFACE-003)
		return cmdPassthrough(cmd, rest, stdout, stderr)
	case "init":
		return cmdInit(rest, stdout, stderr)
	case "status":
		return cmdStatus(rest, stdout, stderr)
	case "models":
		return cmdModels(rest, stdout, stderr)
	case "serve":
		return cmdServe(rest, stdout, stderr)
	case "profile":
		return cmdProfile(rest, stdout, stderr)
	case "provision":
		return cmdProvision(rest, stdout, stderr)
	case "verify-env":
		return cmdVerifyEnv(rest, stdout, stderr)
	case "propose":
		return cmdPropose(rest, stdout, stderr)
	case "frame":
		return cmdFrame(rest, stdout, stderr)
	case "map":
		return cmdMap(rest, stdout, stderr)
	case "version":
		fmt.Fprintf(stdout, "%s (%s)\n", version, commit)
		return 0
	case "-h", "--help", "help":
		usage(stdout)
		return 0
	default:
		fmt.Fprintf(stderr, "aegis: unknown command %q\n", cmd)
		usage(stderr)
		return 2
	}
}

// usage prints the top-level command surface.
func usage(w io.Writer) {
	fmt.Fprint(w, `aegis — air-gap-native agentic coding (OpenCode + local model + rtmx intent)

usage: aegis [command] [flags]

  (no command)  launch the hardened OpenCode TUI (the centerpiece experience)

agent:
  run <prompt>  run one agent task (≡ opencode/ollama run); writes a transcript
  tui           launch the OpenCode TUI explicitly

orchestration (aegis's own):
  loop [--once] [--max N] [--break-after M] [--budget DUR] [--config PATH]
                drain the rtmx backlog (was: aegis run)
  init [--dry-run] [--force] [--config PATH]
                detect host capabilities + write an offline-safe config
  status        unified: config + model endpoint + rtmx backlog
  models        list the local model inventory (loopback endpoint)
  serve         bring the local model server up (calibrated, loopback)
  profile       probe the host + recommend the best-fitting US models (read-only)
  provision [--id X|--browse PATH]   download/source the best-fitting model + serve it
  frame         classify the backlog + surface reframe/unframed lists
  propose <prefix>   emit atomic children of a requirement (human approves)
  verify-env    report egress + traceability status before a run
  version       print the build version

pass-through (hardened; full inner surface):
  rtmx  <args>  forward to rtmx (intent layer)
  code  <args>  forward to opencode (harness)
  model <args>  forward to ollama (local model)
`)
}

// loadConfig resolves config from an optional --config flag value.
func loadConfig(path string, stderr io.Writer) (config.Config, bool) {
	cfg, err := config.Load(path)
	if err != nil {
		fmt.Fprintf(stderr, "aegis: config: %v\n", err)
		return cfg, false
	}
	return cfg, true
}

// cmdInit implements `aegis init`: detect host capabilities, build the install
// plan (target/tier/calibration/offline-safe config), print a readable summary,
// and — unless --dry-run — write the config. Logic lives in internal/install;
// this handler is just flag parsing + I/O.
func cmdInit(args []string, stdout, stderr io.Writer) int {
	fs := flag.NewFlagSet("init", flag.ContinueOnError)
	fs.SetOutput(stderr)
	dryRun := fs.Bool("dry-run", false, "print the plan but write nothing")
	force := fs.Bool("force", false, "overwrite an existing config file")
	cfgPath := fs.String("config", "aegis.json", "config file path to write")
	if err := fs.Parse(args); err != nil {
		return 2
	}

	caps := install.Detect()
	plan := install.Plan(caps)

	// Write the config first (when not a dry run) so the summary reflects what
	// actually happened: an existing-file conflict fails before we claim a write.
	if !*dryRun {
		if err := install.WriteConfig(*cfgPath, plan.Config, *force); err != nil {
			fmt.Fprintf(stderr, "aegis: init: %v\n", err)
			return 1
		}
	}
	if err := install.WritePlan(stdout, plan, *cfgPath, *dryRun); err != nil {
		fmt.Fprintf(stderr, "aegis: init: %v\n", err)
		return 1
	}
	return 0
}

// cmdLoop implements `aegis loop` — drain the rtmx backlog (the orchestration
// loop). Renamed from `aegis run` so `run` can mean a one-shot agent task,
// consistent with opencode/ollama `run` (SURFACE-002).
func cmdLoop(args []string, stdout, stderr io.Writer) int {
	fs := flag.NewFlagSet("loop", flag.ContinueOnError)
	fs.SetOutput(stderr)
	once := fs.Bool("once", false, "run a single iteration")
	maxReq := fs.Int("max", 0, "max requirements this session (0 = config default)")
	breakAfter := fs.Int("break-after", 0, "circuit breaker: halt after M consecutive failures (0 = config default)")
	budget := fs.Duration("budget", 0, "wall-clock budget for this session (0 = config default)")
	cfgPath := fs.String("config", "", "config file path")
	if err := fs.Parse(args); err != nil {
		return 2
	}
	cfg, ok := loadConfig(*cfgPath, stderr)
	if !ok {
		return 1
	}
	if *maxReq > 0 {
		cfg.Budget.MaxRequirements = *maxReq
	}
	if *breakAfter > 0 {
		cfg.BreakAfter = *breakAfter
	}
	if *budget > 0 {
		cfg.Budget.WallClock = *budget
	}
	if err := config.Validate(cfg); err != nil {
		fmt.Fprintf(stderr, "aegis: config: %v\n", err)
		return 1
	}

	// Select the harness adapter the config names (HARNESS-010). The built-in
	// serving-backed harness is constructed here; live rtmx connection + loop
	// drive remain wired in the LOOP/RTMX requirements.
	// RUN-004: refuse to run if the environment is not closed.
	if cfg.AllowEgress {
		fmt.Fprintln(stderr, "aegis: refusing to run: egress is enabled (closed-environment gate)")
		return 1
	}
	adapter, err := selectHarness(cfg)
	if err != nil {
		fmt.Fprintf(stderr, "aegis: harness: %v\n", err)
		return 1
	}
	ctx := context.Background()

	// Build the rtmx client: prefer the MCP stdio server, fall back to CSV/CLI.
	client := buildRTMXClient(ctx, defaultRTMXDB)
	if mc, ok := client.(*rtmx.MCPClient); ok {
		defer mc.Close()
	}
	// RUN-003: audit log to AuditPath (append-only, in-enclave).
	if cfg.AuditPath != "" {
		_ = os.MkdirAll(filepath.Dir(cfg.AuditPath), 0o755)
	}
	auditLog, err := audit.Open(cfg.AuditPath, "aegis-loop")
	if err != nil {
		fmt.Fprintf(stderr, "aegis: audit: %v\n", err)
		return 1
	}

	mode := "drain"
	if *once {
		mode = "once"
	}
	fmt.Fprintf(stdout, "aegis run: mode=%s harness=%s target=%s endpoint=%s\n", mode, adapter.Name(), cfg.Target, cfg.Endpoint)
	_, err = liveRun(ctx, cfg, runDeps{
		RTMX:      client,
		Harness:   adapter,
		Audit:     auditLog,
		Preflight: servingPreflight(cfg.Endpoint, cfg.ModelID, cfg.ModelDigest),
	}, *once, stdout)
	if err != nil {
		fmt.Fprintf(stderr, "aegis: run: %v\n", err)
		return 1
	}
	return 0
}

// defaultRTMXDB is the in-repo rtmx database the loop drives.
const defaultRTMXDB = ".rtmx/database.csv"

// runDeps are the live-run collaborators; production builds them from config,
// tests inject fakes.
type runDeps struct {
	RTMX      rtmx.Client
	Harness   harness.Adapter
	Audit     *audit.Log
	Preflight func(ctx context.Context) error
}

// liveRun executes the control loop with the given dependencies: serving
// preflight (RUN-002), then drive the loop (RUN-001) honoring budget/breaker/park
// (RUN-005), and print a run summary (RUN-003).
func liveRun(ctx context.Context, cfg config.Config, d runDeps, once bool, stdout io.Writer) (loop.Result, error) {
	if d.Preflight != nil {
		if err := d.Preflight(ctx); err != nil {
			return loop.Result{}, fmt.Errorf("serving preflight failed: %w", err)
		}
	}
	lp, err := loop.New(cfg, loop.Deps{
		RTMX:    d.RTMX,
		Harness: d.Harness,
		Audit:   d.Audit,
		Metrics: metrics.NewCollector(),
		Now:     time.Now,
	})
	if err != nil {
		return loop.Result{}, err
	}
	res, err := lp.Run(ctx, once)
	fmt.Fprintf(stdout, "summary: attempted=%d closed=%d parked=%d breaker=%v budget-exhausted=%v\n",
		res.Attempted, res.Closed, res.Parked, res.BreakerTripped, res.BudgetExhausted)
	return res, err
}

// buildRTMXClient prefers the MCP stdio server and falls back to the CSV/CLI
// client when the server cannot be launched.
func buildRTMXClient(ctx context.Context, dbPath string) rtmx.Client {
	if c, err := rtmx.DialMCP(ctx, dbPath); err == nil {
		return c
	}
	return rtmx.NewCLIClient(dbPath)
}

// servingPreflight checks the model endpoint serves completions on loopback and
// (when configured) that the served model matches the expected id/digest.
func servingPreflight(endpoint, expectID, expectDigest string) func(ctx context.Context) error {
	return func(ctx context.Context) error {
		c, err := serving.NewClient(endpoint)
		if err != nil {
			return err
		}
		if err := c.PreflightSmoke(ctx, expectID); err != nil {
			return err
		}
		// SERVE-013: model digest/id gate (skipped when unset).
		if expectID != "" || expectDigest != "" {
			return c.CheckModel(ctx, expectID, expectDigest)
		}
		return nil
	}
}

// cmdPassthrough forwards to an inner tool under the air-gap envelope (SURFACE-003):
// `aegis rtmx|code|model <args>` exec rtmx / opencode / ollama respectively, hiding
// no capability. code/model inherit the hardened launch env (loopback model,
// telemetry/autoupdate off); rtmx runs as-is (it is already local-only).
func cmdPassthrough(ns string, args []string, stdout, stderr io.Writer) int {
	cfg, err := config.Load("")
	if err != nil {
		cfg = config.Default()
	}
	var bin string
	var env []string
	switch ns {
	case "rtmx":
		bin = "rtmx"
	case "model":
		bin = "ollama"
	case "code":
		b, err := opencode.ResolveBinary("")
		if err != nil {
			fmt.Fprintln(stderr, opencode.MissingGuidance)
			return 1
		}
		bin, env = b, opencode.HardenedEnv(cfg)
	}
	path := bin
	if !strings.ContainsRune(bin, '/') {
		p, err := exec.LookPath(bin)
		if err != nil {
			fmt.Fprintf(stderr, "aegis: %s not found on PATH (pass-through for %q)\n", bin, ns)
			return 1
		}
		path = p
	}
	c := exec.Command(path, args...)
	c.Stdin, c.Stdout, c.Stderr = os.Stdin, stdout, stderr
	if env != nil {
		c.Env = append(os.Environ(), env...)
	}
	if err := c.Run(); err != nil {
		var ee *exec.ExitError
		if errors.As(err, &ee) {
			return ee.ExitCode()
		}
		fmt.Fprintf(stderr, "aegis: %s: %v\n", ns, err)
		return 1
	}
	return 0
}

// cmdTUI launches the centerpiece OpenCode TUI under the air-gap-hardened config
// + the local loopback model + rtmx as the intent layer. It loads the config
// from the default path if present, otherwise offline-safe defaults.
func cmdTUI(stdout, stderr io.Writer) int {
	cfg, err := config.Load("")
	if err != nil {
		cfg = config.Default()
	}
	_ = os.Setenv("AEGIS_VERSION", version) // OC-030: the TUI footer shows the aegis version, not OpenCode's
	// Don't spin up a model if the harness isn't even present: a cheap opencode-resolve first means a
	// missing OpenCode guides immediately, and bare `aegis` never loads a 14 GB model just to fail.
	if _, rerr := resolveOpencode(""); rerr != nil {
		fmt.Fprintln(stderr, opencode.MissingGuidance)
		return 1
	}
	// OC-048: show the "Loading aegis…" splash the instant the harness is confirmed present, so the
	// operator sees activity immediately rather than a blank terminal during the model spin-up below.
	// Stopped as soon as the model is up, just before OpenCode paints its TUI.
	stopSplash := opencode.LaunchSplash(stderr)
	// OC-023 ("Auto"): bring the recommended model up before opening the UI, so bare `aegis` just
	// works instead of opening an empty UI that thrashes on "Cannot connect to API". If no model can
	// be resolved, guide the operator to provision one rather than launching an unusable TUI.
	stop, modelID, serr := ensureModelServing(cfg, stderr)
	stopSplash() // model resolution done — clear the splash before any further output / the TUI paint
	if serr != nil {
		if !errors.Is(serr, errNoModel) {
			fmt.Fprintf(stderr, "aegis: %v\n", serr)
			return 1
		}
		// OC-028: no local model — but if the operator already runs Ollama, use those models rather
		// than claiming there are none. Detected + surfaced, with provisioning offered as the alternative.
		if oc, models, ok := ollamaFallback(cfg); ok {
			cfg = oc
			fmt.Fprintf(stderr, "aegis: no local model provisioned — using your Ollama model %q\n", models[0])
			if len(models) > 1 {
				fmt.Fprintf(stderr, "        other Ollama models (switch in the model picker): %s\n", strings.Join(models[1:], ", "))
			}
			fmt.Fprintln(stderr, "        run `aegis provision` for a dedicated, host-calibrated local model instead")
		} else {
			// OC-025: no model is NOT fatal — launch the TUI in the no-model state (the OC-022 screen
			// consumes AEGIS_NO_MODEL to render in-app provisioning) rather than exiting to the shell.
			_ = os.Setenv("AEGIS_NO_MODEL", "1")
			_ = os.Setenv("AEGIS_BEST_FIT", bestFitCard())                    // OC-022: the in-TUI screen shows this
			_ = os.Setenv("AEGIS_MODEL_RATIONALE", recommendationRationale()) // OC-042: why this model is recommended
			if av := bestAvailableModel(); av != nil {
				_ = os.Setenv("AEGIS_AVAILABLE_MODEL", av.ID+" ("+av.Kind+")") // OC-046: best already-present model
				if av.Path != "" {
					_ = os.Setenv("AEGIS_AVAILABLE_PATH", av.Path) // OC-047: the GGUF aegis serves on Ctrl+O
				}
			}
			_ = os.Setenv("AEGIS_MODEL_URL", bestFitURL()) // OC-039: the screen shows the download source URL
			if exe, eerr := os.Executable(); eerr == nil {
				_ = os.Setenv("AEGIS_BIN", exe) // OC-026: the screen spawns this for `aegis provision`
			}
			fmt.Fprintln(stderr, "aegis: no model yet — provision one in the TUI (or run `aegis provision`)")
		}
	}
	if stop != nil {
		defer stop()
	}
	switch { // never leave the picker on the "local-moe" placeholder
	case modelID != "":
		cfg.ModelID = modelID // the actual resource-aware-selected model being served
	case cfg.ModelID == "":
		cfg.ModelID = config.DefaultModelForTarget(cfg.Target)
	}
	// PROFILE-003: profile the host once (cached, ~0.3s after the model is already up) so the gentle
	// best-fit hint can surface at launch — non-intrusive, never blocking.
	autoProfile()
	if h := profileHint(cfg.ModelID); h != "" {
		fmt.Fprintln(stderr, "aegis: "+h)
	}
	if err := tuiLaunch(cfg, "", ""); err != nil {
		if opencode.IsMissing(err) {
			fmt.Fprintln(stderr, opencode.MissingGuidance)
			return 1
		}
		fmt.Fprintf(stderr, "aegis: opencode: %v\n", err)
		return 1
	}
	return 0
}

// endpointReady reports whether the model endpoint answers a loopback completion within timeout.
func endpointReady(endpoint string, timeout time.Duration) bool {
	ctx, cancel := context.WithTimeout(context.Background(), timeout)
	defer cancel()
	return servingPreflight(endpoint, "", "")(ctx) == nil
}

// resolveCalibrationPath finds a host calibration: $AEGIS_CALIBRATION, the user config dir, then
// the repo default. Returns "" when none exists.
func resolveCalibrationPath() string {
	if p := os.Getenv("AEGIS_CALIBRATION"); p != "" {
		return p
	}
	var cands []string
	if home, err := os.UserHomeDir(); err == nil {
		cands = append(cands, filepath.Join(home, ".config", "aegis", "calibration.json"))
	}
	cands = append(cands, "deploy/llama-server/calibration.json")
	for _, c := range cands {
		if _, err := os.Stat(c); err == nil {
			return c
		}
	}
	return ""
}

// resolveAnyModelGGUF returns a side-loaded model GGUF for the fast-start path, scanning the model
// download dir ($MODEL_DOWNLOAD_DIR, else ~/models). It returns the SMALLEST *.gguf — the fastest to
// load + most interactive, honoring "get started quickly"; the background profiler later recommends
// the best-fitting model. Returns "" when none is present.
func resolveAnyModelGGUF() string {
	dir := os.Getenv("MODEL_DOWNLOAD_DIR")
	if dir == "" {
		home, err := os.UserHomeDir()
		if err != nil {
			return ""
		}
		dir = filepath.Join(home, "models")
	}
	entries, err := os.ReadDir(dir)
	if err != nil {
		return ""
	}
	best, bestSize := "", int64(0)
	for _, e := range entries {
		if e.IsDir() || !strings.HasSuffix(e.Name(), ".gguf") {
			continue
		}
		info, err := e.Info()
		if err != nil {
			continue
		}
		if best == "" || info.Size() < bestSize {
			best, bestSize = filepath.Join(dir, e.Name()), info.Size()
		}
	}
	return best
}

// writeSeedCalibration synthesizes a conservative, host-shaped calibration for gguf (internal/install
// seed: threads≈physical cores, ngl per target) and writes it to the user config dir, so the launch
// serves immediately and the background profiler / bench.sh can refine the SAME file later. Returns
// the written path.
func writeSeedCalibration(gguf string) (string, error) {
	cal := install.Plan(install.Detect()).Calibration
	cal.Model = gguf
	if n := catalogCtxSizeForGGUF(gguf); n > 0 {
		cal.CtxSize = n
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	dir := filepath.Join(home, ".config", "aegis")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return "", err
	}
	path := filepath.Join(dir, "calibration.json")
	b, err := json.MarshalIndent(cal, "", "  ")
	if err != nil {
		return "", err
	}
	if err := os.WriteFile(path, b, 0o644); err != nil {
		return "", err
	}
	return path, nil
}

// provisionGuidance is the operator-facing message when no model can be brought up (OC-023). It is
// resource-aware: aegis serves the largest model the host can hold responsively (internal/install
// Plan picks the envelope by RAM) — not every model fits every system. The air-gap rule forbids
// silent downloads, so we tell the operator how to provision the right-sized model.
func provisionGuidance() string {
	caps := install.Detect()
	plan := install.Plan(caps)
	return fmt.Sprintf("no local model is provisioned.\n"+
		"  This host (%d GiB RAM, %d cores) best fits the %s model envelope — aegis serves the\n"+
		"  largest model your system can hold responsively; not every model fits every system.\n"+
		"  Provision a model in that envelope, then re-run aegis:\n"+
		"    • download (connected host):  scripts/fetch-model.sh <catalog-id>\n"+
		"    • or source a local GGUF:     scripts/pin-model.sh <path-to-model.gguf>\n"+
		"    • then calibrate this host:    scripts/bench.sh\n"+
		"  (US-origin gemma-4-26b-a4b is the default that fits the minimum envelope.)",
		caps.TotalRAMGB(), caps.PhysicalCPU, plan.Tier)
}

// ensureModelServing makes a model available on cfg.Endpoint before the TUI opens (OC-023, "Auto").
// If one is already serving it returns immediately. Otherwise it resolves a calibration + its model
// GGUF — the resource-aware model chosen at provision time (internal/install Plan) — and brings
// llama-server up in the background, streaming progress, returning a stop func + the served model
// id. If it cannot, it returns a resource-aware guidance error rather than opening an unusable UI.
func ensureModelServing(cfg config.Config, out io.Writer) (stop func(), modelID string, err error) {
	if endpointReady(cfg.Endpoint, 12*time.Second) {
		return nil, servingModelID(), nil // already serving — label it with the catalog id (picker + hint)
	}
	calPath := resolveCalibrationPath()
	if calPath == "" {
		// Fast-start guarantee: never block the operator on a full bench. If a model GGUF is already
		// side-loaded, start it NOW with conservative, host-shaped default tuning (internal/install
		// seed calibration); the background profiler / bench.sh refine it later. Only when no model
		// is present at all do we fall back to provisioning guidance.
		gguf := resolveAnyModelGGUF()
		if gguf == "" {
			return nil, "", noModelErr()
		}
		seed, werr := writeSeedCalibration(gguf)
		if werr != nil {
			return nil, "", noModelErr()
		}
		fmt.Fprintf(out, "aegis: no calibration yet — starting %s with default tuning (run `aegis profile` to tune for this host)\n", filepath.Base(gguf))
		calPath = seed
	}
	cal, lerr := serving.LoadCalibration(calPath)
	if lerr != nil {
		return nil, "", fmt.Errorf("calibration %s: %w", calPath, lerr)
	}
	if cal.Model == "" {
		return nil, "", noModelErr()
	}
	if _, serr := os.Stat(cal.Model); serr != nil {
		return nil, "", fmt.Errorf("%w (the calibrated model is not present: %s)", errNoModel, cal.Model)
	}
	fmt.Fprintf(out, "aegis: no model running — bringing up %s (loopback)\n", filepath.Base(cal.Model))
	cmd, berr := buildServeCommand(calPath)
	if berr != nil {
		return nil, "", berr
	}
	cmd.Stdout, cmd.Stderr = io.Discard, io.Discard // the model loads in the background
	if startErr := cmd.Start(); startErr != nil {
		return nil, "", fmt.Errorf("launch model server: %w", startErr)
	}
	stop = func() {
		if cmd.Process != nil {
			_ = cmd.Process.Kill()
			_ = cmd.Wait()
		}
	}
	fmt.Fprint(out, "aegis: loading model ")
	deadline := time.Now().Add(180 * time.Second)
	for time.Now().Before(deadline) {
		if endpointReady(cfg.Endpoint, 4*time.Second) {
			fmt.Fprintln(out, "— ready")
			return stop, catalogIDForGGUF(cal.Model), nil
		}
		fmt.Fprint(out, ".")
		time.Sleep(2 * time.Second)
	}
	stop()
	return nil, "", fmt.Errorf("model did not become ready within 180s (check the model + calibration)")
}

// servingModelID resolves the catalog id of the model the active calibration points at, so an
// already-running endpoint is labeled consistently with the profiler/catalog (not the ollama-tag
// fallback). "" when no calibration or no catalog match.
func servingModelID() string {
	if cp := resolveCalibrationPath(); cp != "" {
		if cal, err := serving.LoadCalibration(cp); err == nil {
			return catalogIDForGGUF(cal.Model)
		}
	}
	return ""
}

// catalogIDForGGUF returns the catalog model id whose GGUF file matches ggufPath's basename — so the
// picker shows the real, resource-aware-selected model rather than the "local-moe" placeholder — or
// "" when the catalog or a match is absent.
func catalogIDForGGUF(ggufPath string) string {
	base := filepath.Base(ggufPath)
	b, err := catalogBytes() // embedded-aware: an installed binary has no catalog file (PERF-007)
	if err != nil {
		return ""
	}
	var cat struct {
		Models []struct {
			ID   string `json:"id"`
			File string `json:"file"`
		} `json:"models"`
	}
	if json.Unmarshal(b, &cat) != nil {
		return ""
	}
	for _, m := range cat.Models {
		if m.File == base {
			return m.ID
		}
	}
	return ""
}

// cmdRun implements `aegis run <prompt>` (SURFACE-002 / BENCH-001): a one-shot
// headless agent task — drives the classic `opencode run` against the local model
// in a workdir and writes an intent-bench transcript. Consistent with
// opencode/ollama `run`. (`aegis solve` is a back-compat alias.)
// runSolve is the seam between cmdRun and the headless serve drive, overridable in
// tests so the command's transcript-writing path is unit-testable without a real
// OpenCode binary (the real drive is covered by the gated serve-drive integration).
var runSolve = opencode.Solve

// loadCatalogTuning resolves the model catalog (alongside the aegis binary, then
// cwd-relative deploy/models/catalog.json) and returns the per-model serving tuning
// for modelID (SERVE-020), or nil when the catalog or a match is absent.
func loadCatalogTuning(modelID string) *config.ModelTuning {
	if modelID == "" {
		return nil
	}
	if b, err := catalogBytes(); err == nil { // embedded-aware (PERF-007)
		return config.TuningForModel(modelID, b)
	}
	return nil
}

// catalogCtxSizeForGGUF returns the catalog tuning's num_ctx for a GGUF model path
// (SERVE-017), or 0 when the catalog or a match is absent.
func catalogCtxSizeForGGUF(ggufPath string) int {
	if ggufPath == "" {
		return 0
	}
	if b, err := catalogBytes(); err == nil { // embedded-aware (PERF-007)
		if t := config.TuningForGGUF(ggufPath, b); t != nil && t.NumCtx != nil {
			return *t.NumCtx
		}
	}
	return 0
}

// deployFileBytes reads a deploy-relative file, looking alongside the aegis binary first,
// then cwd-relative.
func deployFileBytes(rel string) ([]byte, error) {
	if self, err := os.Executable(); err == nil {
		if b, err := os.ReadFile(filepath.Join(filepath.Dir(self), rel)); err == nil {
			return b, nil
		}
	}
	if b, err := os.ReadFile(rel); err == nil {
		return b, nil
	}
	if b, ok := embeddedDeploy[filepath.ToSlash(rel)]; ok { // OC-033: embedded default for an installed aegis
		return b, nil
	}
	return os.ReadFile(rel)
}

// originPolicyPath resolves the origin policy file: AEGIS_ORIGIN_POLICY if set, else the
// deploy file (alongside the binary, then cwd).
func originPolicyPath() string {
	if p := os.Getenv("AEGIS_ORIGIN_POLICY"); p != "" {
		return p
	}
	if self, err := os.Executable(); err == nil {
		p := filepath.Join(filepath.Dir(self), origin.DefaultPolicyPath)
		if _, err := os.Stat(p); err == nil {
			return p
		}
	}
	return origin.DefaultPolicyPath
}

// verifyModelOrigin enforces the model-origin policy (MODEL-007): it resolves the pinned
// model (MODEL_REF) + the catalog + the policy and fails when the origin is not allowed.
// Absent MODEL_REF or catalog is a SKIP (nothing to gate), not a failure.
func verifyModelOrigin(stdout io.Writer) int {
	refBytes, err := deployFileBytes(filepath.Join("deploy", "models", "MODEL_REF"))
	if err != nil {
		fmt.Fprintf(stdout, "origin=SKIP (no MODEL_REF pinned)\n")
		return 0
	}
	var ref struct {
		Name string `json:"name"`
	}
	if err := json.Unmarshal(refBytes, &ref); err != nil || ref.Name == "" {
		fmt.Fprintf(stdout, "origin=SKIP (MODEL_REF unreadable)\n")
		return 0
	}
	catalog, err := catalogBytes()
	if err != nil {
		fmt.Fprintf(stdout, "origin=SKIP (no model catalog)\n")
		return 0
	}
	pol, err := aegisOriginPolicy()
	if err != nil {
		fmt.Fprintf(stdout, "origin=FAIL (policy: %v)\n", err)
		return 1
	}
	country, known := origin.OriginForModel(ref.Name, catalog)
	label := "unknown"
	if known {
		label = country
	}
	if err := origin.CheckModel(ref.Name, catalog, pol); err != nil {
		fmt.Fprintf(stdout, "origin=FAIL model=%s origin=%s (%v)\n", ref.Name, label, err)
		return 1
	}
	fmt.Fprintf(stdout, "origin=OK model=%s origin=%s (policy-allowed)\n", ref.Name, label)
	return 0
}

func cmdRun(args []string, stdout, stderr io.Writer) int {
	fs := flag.NewFlagSet("run", flag.ContinueOnError)
	fs.SetOutput(stderr)
	workdir := fs.String("workdir", ".", "project directory the agent works in")
	promptFile := fs.String("prompt-file", "", "file with the task prompt (or use --prompt)")
	promptStr := fs.String("prompt", "", "the task prompt (inline)")
	model := fs.String("model", "", "model id (defaults to config model_id)")
	out := fs.String("out", "", "write the intent-bench transcript here (default stdout)")
	timeout := fs.Duration("timeout", 5*time.Minute, "wall-clock budget for the run (RUNQ-001)")
	noIntent := fs.Bool("no-intent", false, "omit the rtmx MCP intent layer (intent-bench control condition)")
	cfgPath := fs.String("config", "", "config file path")
	if err := fs.Parse(args); err != nil {
		return 2
	}
	prompt := *promptStr
	if *promptFile != "" {
		b, err := os.ReadFile(*promptFile)
		if err != nil {
			fmt.Fprintf(stderr, "aegis: run: %v\n", err)
			return 1
		}
		prompt = string(b)
	}
	if strings.TrimSpace(prompt) == "" {
		fmt.Fprintln(stderr, "aegis: run: a --prompt or --prompt-file is required")
		return 2
	}
	cfg, ok := loadConfig(*cfgPath, stderr)
	if !ok {
		cfg = config.Default()
	}
	if cfg.AllowEgress {
		fmt.Fprintln(stderr, "aegis: refusing to run: egress is enabled (closed-environment gate)")
		return 1
	}

	// Target-aware default model when a run names none (RUNQ-004): on linux-cpu the
	// CPU-capable completer (gemma4-qat) is the default — the qwen3-coder bundle default
	// fast-fails on CPU (its Ollama tag emits Qwen-native XML tool calls that leak as text,
	// and runs at Ollama's small default context). On darwin-metal qwen3-coder is the default.
	effModel := *model
	if effModel == "" {
		effModel = cfg.ModelID
	}
	if effModel == "" {
		effModel = config.DefaultModelForTarget(cfg.Target)
		cfg.ModelID = effModel // the serve-drive falls back to cfg.ModelID for the run
	}
	// SERVE-020: apply the per-model serving tuning from the catalog (unless the
	// operator set it explicitly), so the launched model emits reliable tool calls.
	if cfg.Tuning == nil {
		cfg.Tuning = loadCatalogTuning(effModel)
	}

	// RUNQ-003: bound a capable-but-rambling model with step/output limits (defaults
	// applied unless the config sets them) so the run completes instead of running away.
	if cfg.MaxSteps == 0 {
		cfg.MaxSteps = config.DefaultMaxSteps
	}
	if cfg.MaxOutputTokens == 0 {
		cfg.MaxOutputTokens = config.DefaultMaxOutputTokens
	}

	// RUNQ-001: bound the run by a wall-clock budget; a partial transcript is still
	// written on timeout.
	ctx, cancel := context.WithTimeout(context.Background(), *timeout)
	defer cancel()
	res, err := runSolve(ctx, cfg, "", opencode.SolveOptions{
		Workdir: *workdir, Prompt: prompt, Model: *model, NoIntent: *noIntent,
	})
	if err != nil {
		if opencode.IsMissing(err) {
			fmt.Fprintln(stderr, opencode.MissingGuidance)
			return 1
		}
		fmt.Fprintf(stderr, "aegis: run: %v\n", err)
		return 1
	}

	w := stdout
	if *out != "" {
		f, err := os.Create(*out)
		if err != nil {
			fmt.Fprintf(stderr, "aegis: run: %v\n", err)
			return 1
		}
		defer f.Close()
		w = f
	}
	if err := bench.WriteTranscript(w, res.Messages); err != nil {
		fmt.Fprintf(stderr, "aegis: run: transcript: %v\n", err)
		return 1
	}
	if res.TimedOut {
		fmt.Fprintf(stderr, "aegis: run: timed out after %s — partial transcript (%d messages) written\n", *timeout, len(res.Messages))
		return 124
	}
	fmt.Fprintf(stderr, "aegis: run: %d messages\n", len(res.Messages))
	return 0
}

// cmdFrame implements `aegis frame`: classify the backlog and surface the
// continuous-discovery evidence (the reframe backlog + framing-hygiene gaps).
// It is assistive — it reports; a human reframes and approves.
func cmdFrame(args []string, stdout, stderr io.Writer) int {
	fs := flag.NewFlagSet("frame", flag.ContinueOnError)
	fs.SetOutput(stderr)
	if err := fs.Parse(args); err != nil {
		return 2
	}
	reqs, err := rtmx.NewStore(defaultRTMXDB).Requirements()
	if err != nil {
		fmt.Fprintf(stderr, "aegis: frame: %v\n", err)
		return 1
	}
	frameReport(reqs, stdout)
	return 0
}

// frameReport prints the five-way classification plus the reframe and unframed
// lists. Separated from cmdFrame so it is testable without a database.
func frameReport(reqs []*rtmx.Requirement, w io.Writer) {
	c := framing.Classify(reqs)
	fmt.Fprintf(w, "backlog: delivered=%d in-flight=%d parked=%d proposed=%d (unframed=%d)\n",
		len(c.Delivered), len(c.InFlight), len(c.Parked), len(c.Proposed), len(c.Unframed))
	if len(c.Parked) > 0 {
		fmt.Fprintf(w, "reframe backlog (parked — discovery input): %s\n", strings.Join(c.Parked, ", "))
	}
	if len(c.Unframed) > 0 {
		fmt.Fprintf(w, "unframed (needs a spec/outcome trace): %s\n", strings.Join(c.Unframed, ", "))
	}
}

// cmdStatus implements `aegis status`.
func cmdStatus(args []string, stdout, stderr io.Writer) int {
	fs := flag.NewFlagSet("status", flag.ContinueOnError)
	fs.SetOutput(stderr)
	cfgPath := fs.String("config", "", "config file path")
	if err := fs.Parse(args); err != nil {
		return 2
	}
	cfg, ok := loadConfig(*cfgPath, stderr)
	if !ok {
		return 1
	}
	fmt.Fprintf(stdout, "endpoint=%s harness=%s target=%s audit=%s\n",
		cfg.Endpoint, cfg.Harness, cfg.Target, cfg.AuditPath)
	// SURFACE-004: unify health/inventory across the stack — the model endpoint
	// and the rtmx intent backlog, alongside config.
	if c, err := serving.NewClient(cfg.Endpoint); err == nil {
		ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
		defer cancel()
		if mi, err := c.ModelInfo(ctx); err == nil {
			fmt.Fprintf(stdout, "model: %s (endpoint reachable)\n", mi.ID)
		} else {
			fmt.Fprintf(stdout, "model: endpoint unreachable (%s)\n", cfg.Endpoint)
		}
	}
	if total, complete, err := rtmxCounts(defaultRTMXDB); err == nil {
		fmt.Fprintf(stdout, "rtmx: %d/%d requirements complete\n", complete, total)
	}
	return 0
}

// rtmxCounts reads the rtmx CSV and returns total + COMPLETE requirement counts.
func rtmxCounts(db string) (total, complete int, err error) {
	f, err := os.Open(db)
	if err != nil {
		return 0, 0, err
	}
	defer f.Close()
	rows, err := csv.NewReader(f).ReadAll()
	if err != nil || len(rows) < 1 {
		return 0, 0, err
	}
	statusCol := -1
	for i, hdr := range rows[0] {
		if hdr == "status" {
			statusCol = i
		}
	}
	for _, row := range rows[1:] {
		total++
		if statusCol >= 0 && statusCol < len(row) && row[statusCol] == "COMPLETE" {
			complete++
		}
	}
	return total, complete, nil
}

// cmdModels implements `aegis models` (SURFACE-004): the local model inventory,
// queried from the configured loopback endpoint (hardened — loopback only).
func cmdModels(args []string, stdout, stderr io.Writer) int {
	fs := flag.NewFlagSet("models", flag.ContinueOnError)
	fs.SetOutput(stderr)
	cfgPath := fs.String("config", "", "config file path")
	if err := fs.Parse(args); err != nil {
		return 2
	}
	cfg, ok := loadConfig(*cfgPath, stderr)
	if !ok {
		return 1
	}
	c, err := serving.NewClient(cfg.Endpoint)
	if err != nil {
		fmt.Fprintf(stderr, "aegis: models: %v\n", err)
		return 1
	}
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	models, err := c.Models(ctx)
	if err != nil {
		fmt.Fprintf(stderr, "aegis: models: %s unreachable: %v\n", cfg.Endpoint, err)
		return 1
	}
	for _, m := range models {
		if m.Digest != "" {
			fmt.Fprintf(stdout, "%s\t%s\n", m.ID, m.Digest)
		} else {
			fmt.Fprintln(stdout, m.ID)
		}
	}
	return 0
}

// cmdServe implements `aegis serve` (SURFACE-004): bring the local model server up
// on loopback under the calibrated launch args (internal/serving.LaunchArgs), with
// the self-built llama-server resolved from deploy/llama-server/bin.
func cmdServe(args []string, stdout, stderr io.Writer) int {
	fs := flag.NewFlagSet("serve", flag.ContinueOnError)
	fs.SetOutput(stderr)
	calPath := fs.String("calibration", "deploy/llama-server/calibration.json", "calibration file")
	if err := fs.Parse(args); err != nil {
		return 2
	}
	cmd, err := buildServeCommand(*calPath)
	if err != nil {
		fmt.Fprintf(stderr, "aegis: serve: %v\n", err)
		return 1
	}
	fmt.Fprintf(stderr, "aegis: serve: launching %s (loopback)\n", cmd.Path)
	cmd.Stdout, cmd.Stderr = stdout, stderr
	if err := cmd.Run(); err != nil {
		fmt.Fprintf(stderr, "aegis: serve: %v\n", err)
		return 1
	}
	return 0
}

// buildServeCommand builds the calibrated llama-server launch command, resolving
// the self-built binary. Separated for testing.
func buildServeCommand(calPath string) (*exec.Cmd, error) {
	cal, err := serving.LoadCalibration(calPath)
	if err != nil {
		return nil, fmt.Errorf("calibration: %w (run scripts/bench.sh)", err)
	}
	// SERVE-017/020: if the calibration sets no ctx_size, carry the selected model's
	// num_ctx from the catalog (matched by GGUF file) onto --ctx-size, so the production
	// path serves the tuned context robustly instead of llama.cpp's small default.
	if cal.CtxSize == 0 {
		if n := catalogCtxSizeForGGUF(cal.Model); n > 0 {
			cal.CtxSize = n
		}
	}
	argv, err := serving.LaunchArgs(cal)
	if err != nil {
		return nil, err
	}
	bin := resolveLlamaServer()
	for i, a := range argv {
		if a == "llama-server" {
			argv[i] = bin
			break
		}
	}
	if len(argv) == 0 {
		return nil, fmt.Errorf("empty launch command")
	}
	return exec.Command(argv[0], argv[1:]...), nil
}

// resolveLlamaServer prefers the self-built binary, then PATH.
func resolveLlamaServer() string {
	staged := "deploy/llama-server/bin/llama-server"
	if fi, err := os.Stat(staged); err == nil && fi.Mode().Perm()&0o111 != 0 {
		if abs, err := filepath.Abs(staged); err == nil {
			return abs
		}
		return staged
	}
	// REL-005: package install layout (e.g. /usr/lib/aegis/llama-server).
	for _, d := range opencode.LibexecDirs() {
		p := filepath.Join(d, "llama-server")
		if fi, err := os.Stat(p); err == nil && fi.Mode().Perm()&0o111 != 0 {
			return p
		}
	}
	if p, err := exec.LookPath("llama-server"); err == nil {
		return p
	}
	return "llama-server"
}

// cmdVerifyEnv implements `aegis verify-env`: it reports whether the
// environment is closed (loopback-only) and traceable before a real run.
func cmdVerifyEnv(args []string, stdout, stderr io.Writer) int {
	fs := flag.NewFlagSet("verify-env", flag.ContinueOnError)
	fs.SetOutput(stderr)
	cfgPath := fs.String("config", "", "config file path")
	checkOpenCode := fs.Bool("check-opencode", false, "also launch OpenCode under the hardened env and confirm it bootstraps closed (loopback-only); run this under scripts/verify-airgap.sh for the whole-group EGRESS=0 proof (ENCLAVE-001)")
	checkOrigin := fs.Bool("check-origin", false, "enforce the model-origin policy (MODEL-007): fail if the pinned model (MODEL_REF) has an origin not allowed by deploy/models/origin-policy.json")
	if err := fs.Parse(args); err != nil {
		return 2
	}
	cfg, ok := loadConfig(*cfgPath, stderr)
	if !ok {
		// Validation failure (e.g. non-loopback endpoint) is a closed-env failure.
		fmt.Fprintln(stdout, "egress=FAIL trace=UNKNOWN")
		return 1
	}
	// Offline-safe config validated: egress is loopback-only by construction.
	egress := "OK"
	if cfg.AllowEgress {
		egress = "FAIL"
	}
	fmt.Fprintf(stdout, "egress=%s endpoint=%s harness=%s\n", egress, cfg.Endpoint, cfg.Harness)
	if egress != "OK" {
		return 1
	}
	if *checkOrigin {
		if rc := verifyModelOrigin(stdout); rc != 0 {
			return rc
		}
	}
	if *checkOpenCode {
		// The launch check proves EGRESS=0 only for a COMPLETE bundle. If OpenCode or
		// its bundled ripgrep is not staged, the bootstrap would reach for the network
		// (a download), so we cannot prove loopback-only — skip with a loud note rather
		// than a false pass or false fail. Bundle completeness is OC-009/REL's concern.
		if _, err := opencode.ResolveBinary(""); err != nil {
			fmt.Fprintln(stdout, "opencode=SKIP (OpenCode not staged; bundle it to enforce the launch check)")
			return 0
		}
		if _, ok := opencode.ResolveRipgrep(); !ok {
			if _, err := exec.LookPath("rg"); err != nil {
				fmt.Fprintln(stdout, "opencode=SKIP (ripgrep not staged; run scripts/stage-ripgrep.sh to enforce the launch check)")
				return 0
			}
		}
		ctx, cancel := context.WithTimeout(context.Background(), 90*time.Second)
		defer cancel()
		if err := opencode.VerifyLaunch(ctx, cfg); err != nil {
			fmt.Fprintf(stdout, "opencode=FAIL (bootstrap did not reach readiness: %v)\n", err)
			return 1
		}
		fmt.Fprintln(stdout, "opencode=OK (bootstrapped to readiness, loopback-only)")
	}
	return 0
}

// cmdPropose implements `aegis propose <prefix>`: it reports the human-gated
// decomposition entrypoint. Approval is never automatic.
func cmdPropose(args []string, stdout, stderr io.Writer) int {
	fs := flag.NewFlagSet("propose", flag.ContinueOnError)
	fs.SetOutput(stderr)
	if err := fs.Parse(args); err != nil {
		return 2
	}
	if fs.NArg() < 1 {
		fmt.Fprintln(stderr, "aegis: propose requires a <prefix> argument")
		return 2
	}
	prefix := fs.Arg(0)
	fmt.Fprintf(stdout, "aegis propose: prefix=%s (children land in 'proposed' state; a human approves)\n", prefix)
	return 0
}
