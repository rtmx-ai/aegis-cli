// Command aegis is the aegis-cli orchestrator entrypoint.
//
// It is a thin dispatcher over the internal packages: subcommands are run,
// status, verify-env, propose and version. Real logic lives in internal/.
package main

import (
	"context"
	"flag"
	"fmt"
	"io"
	"os"
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
	case "code", "tui":
		return cmdTUI(stdout, stderr)
	case "solve":
		return cmdSolve(rest, stdout, stderr)
	case "init":
		return cmdInit(rest, stdout, stderr)
	case "run":
		return cmdRun(rest, stdout, stderr)
	case "status":
		return cmdStatus(rest, stdout, stderr)
	case "verify-env":
		return cmdVerifyEnv(rest, stdout, stderr)
	case "propose":
		return cmdPropose(rest, stdout, stderr)
	case "frame":
		return cmdFrame(rest, stdout, stderr)
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
	fmt.Fprint(w, `aegis — air-gap-native agentic coding (OpenCode TUI + local model + rtmx intent)

usage: aegis [command] [flags]

  (no command)  launch the bundled OpenCode TUI (the centerpiece experience)

commands:
  code | tui    launch the OpenCode TUI explicitly
  init [--dry-run] [--force] [--config PATH]
                detect host capabilities, plan target/tier/calibration, and
                write an offline-safe config (then calibrate + verify air-gap)
  run [--once] [--max N] [--break-after M] [--budget DUR] [--config PATH]
                drain the backlog (or one iteration with --once)
  status        report backlog + endpoint status
  verify-env    report egress + traceability status before a run
  propose <prefix>
                emit atomic children of a requirement (human approves)
  frame         classify the backlog + surface the reframe/unframed lists
                (continuous-discovery evidence; assistive, human reframes)
  version       print the build version
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

// cmdRun implements `aegis run`.
func cmdRun(args []string, stdout, stderr io.Writer) int {
	fs := flag.NewFlagSet("run", flag.ContinueOnError)
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

// cmdTUI launches the centerpiece OpenCode TUI under the air-gap-hardened config
// + the local loopback model + rtmx as the intent layer. It loads the config
// from the default path if present, otherwise offline-safe defaults.
func cmdTUI(stdout, stderr io.Writer) int {
	cfg, err := config.Load("")
	if err != nil {
		cfg = config.Default()
	}
	if err := opencode.Launch(cfg, "", ""); err != nil {
		if opencode.IsMissing(err) {
			fmt.Fprintln(stderr, opencode.MissingGuidance)
			return 1
		}
		fmt.Fprintf(stderr, "aegis: opencode: %v\n", err)
		return 1
	}
	return 0
}

// cmdSolve implements `aegis solve` (BENCH-001): a headless agent run that drives
// OpenCode's serve API to autonomously complete a prompt in a workdir against the
// local model, then writes an intent-bench transcript. This is the benchmarkable
// surface (and the proof the local stack actually codes).
func cmdSolve(args []string, stdout, stderr io.Writer) int {
	fs := flag.NewFlagSet("solve", flag.ContinueOnError)
	fs.SetOutput(stderr)
	workdir := fs.String("workdir", ".", "project directory the agent works in")
	promptFile := fs.String("prompt-file", "", "file with the task prompt (or use --prompt)")
	promptStr := fs.String("prompt", "", "the task prompt (inline)")
	model := fs.String("model", "", "model id (defaults to config model_id)")
	out := fs.String("out", "", "write the intent-bench transcript here (default stdout)")
	port := fs.Int("port", 8099, "loopback port for the opencode serve API")
	cfgPath := fs.String("config", "", "config file path")
	if err := fs.Parse(args); err != nil {
		return 2
	}
	prompt := *promptStr
	if *promptFile != "" {
		b, err := os.ReadFile(*promptFile)
		if err != nil {
			fmt.Fprintf(stderr, "aegis: solve: %v\n", err)
			return 1
		}
		prompt = string(b)
	}
	if strings.TrimSpace(prompt) == "" {
		fmt.Fprintln(stderr, "aegis: solve: a --prompt or --prompt-file is required")
		return 2
	}
	cfg, ok := loadConfig(*cfgPath, stderr)
	if !ok {
		cfg = config.Default()
	}
	if cfg.AllowEgress {
		fmt.Fprintln(stderr, "aegis: refusing to solve: egress is enabled (closed-environment gate)")
		return 1
	}

	res, err := opencode.Solve(context.Background(), cfg, "", opencode.SolveOptions{
		Workdir: *workdir, Prompt: prompt, Model: *model, Port: *port,
	})
	if err != nil {
		if opencode.IsMissing(err) {
			fmt.Fprintln(stderr, opencode.MissingGuidance)
			return 1
		}
		fmt.Fprintf(stderr, "aegis: solve: %v\n", err)
		return 1
	}

	w := stdout
	if *out != "" {
		f, err := os.Create(*out)
		if err != nil {
			fmt.Fprintf(stderr, "aegis: solve: %v\n", err)
			return 1
		}
		defer f.Close()
		w = f
	}
	if err := bench.WriteTranscript(w, res.Messages); err != nil {
		fmt.Fprintf(stderr, "aegis: solve: transcript: %v\n", err)
		return 1
	}
	fmt.Fprintf(stderr, "aegis: solve: session %s, %d messages\n", res.SessionID, len(res.Messages))
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
	return 0
}

// cmdVerifyEnv implements `aegis verify-env`: it reports whether the
// environment is closed (loopback-only) and traceable before a real run.
func cmdVerifyEnv(args []string, stdout, stderr io.Writer) int {
	fs := flag.NewFlagSet("verify-env", flag.ContinueOnError)
	fs.SetOutput(stderr)
	cfgPath := fs.String("config", "", "config file path")
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
