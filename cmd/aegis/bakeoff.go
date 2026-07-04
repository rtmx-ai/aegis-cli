package main

import (
	"bufio"
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"github.com/rtmx-ai/aegis-cli/internal/bakeoff"
	"github.com/rtmx-ai/aegis-cli/internal/install"
	"github.com/rtmx-ai/aegis-cli/internal/serving"
)

// defaultSuite is the fixed bake-off task set (BENCH-010): small, deterministic Go tasks that each fail
// until the model makes the right change, verified by `go test`. Two EDITS and one CREATE (a new file) so
// the suite exercises both the edit and write tools — the minimum agentic coding bar, with objective
// pass/fail. Go so scoring needs no extra runtime. Extend as the bake-off matures.
func defaultSuite() []bakeoff.Task {
	gomod := "module task\n\ngo 1.21\n"
	return []bakeoff.Task{
		{
			Name:   "go-add",
			Prompt: "Edit add.go so that Add(a, b) returns a + b instead of 0. Make the change with the edit tool.",
			Files: map[string]string{
				"go.mod":      gomod,
				"add.go":      "package task\n\nfunc Add(a, b int) int { return 0 }\n",
				"add_test.go": "package task\n\nimport \"testing\"\n\nfunc TestAdd(t *testing.T){ if Add(2,3)!=5 { t.Fatal(\"want 5\") } }\n",
			},
			Verify: []string{"go", "test", "./..."},
		},
		{
			Name:   "go-max",
			Prompt: "Edit max.go so that Max(a, b) returns the larger of a and b (it currently returns 0). Use the edit tool.",
			Files: map[string]string{
				"go.mod":      gomod,
				"max.go":      "package task\n\nfunc Max(a, b int) int { return 0 }\n",
				"max_test.go": "package task\n\nimport \"testing\"\n\nfunc TestMax(t *testing.T){ if Max(2,7)!=7 || Max(9,4)!=9 { t.Fatal(\"wrong\") } }\n",
			},
			Verify: []string{"go", "test", "./..."},
		},
		{
			Name:   "go-greet",
			Prompt: "Create a new file greet.go in package task with a function Greet(name string) string that returns \"Hello, \" + name + \"!\". Use the write tool to create the file.",
			Files: map[string]string{
				"go.mod":        gomod,
				"greet_test.go": "package task\n\nimport \"testing\"\n\nfunc TestGreet(t *testing.T){ if Greet(\"Ada\")!=\"Hello, Ada!\" { t.Fatal(\"wrong greeting\") } }\n",
			},
			Verify: []string{"go", "test", "./..."},
		},
	}
}

// cmdBakeoff runs the fixed suite across candidate models on the real serve→opencode→verify path and
// writes a head-to-head comparison (agency + throughput). Each candidate must already be reachable at the
// endpoint (serve it via `aegis serve`/provision on the target); the rig drives `aegis run` per cell.
func cmdBakeoff(args []string, stdout, stderr io.Writer) int {
	fs := flag.NewFlagSet("bakeoff", flag.ContinueOnError)
	fs.SetOutput(stderr)
	all := fs.Bool("all", false, "bake off ALL host-suitable models, downloading any that are missing")
	models := fs.String("models", "", "explicit comma-separated model ids (overrides the host auto-select)")
	noServe := fs.Bool("no-serve", false, "don't serve models; measure whatever is already at --endpoint (needs --models)")
	noDownload := fs.Bool("no-download", false, "only bake off models already present locally (never download)")
	endpoint := fs.String("endpoint", "http://127.0.0.1:8080", "loopback endpoint aegis serves each candidate on")
	timeout := fs.Duration("timeout", 300*time.Second, "per-task wall-clock budget")
	outPath := fs.String("out", "eval/bakeoff/comparison.json", "where to write the comparison JSON")
	host := fs.String("host", "", "host label for the report (default: the `aegis profile` target)")
	quiet := fs.Bool("quiet", false, "quiet: only the final result table (and any errors)")
	fs.BoolVar(quiet, "q", false, "quiet (shorthand)")
	verbose := fs.Bool("verbose", false, "verbose: also show serving detail, served paths, and full per-cell errors")
	fs.BoolVar(verbose, "v", false, "verbose (shorthand)")
	noColor := fs.Bool("no-color", false, "disable ANSI color (also honors $NO_COLOR)")
	if err := fs.Parse(args); err != nil {
		return 2
	}
	// Observability level: 0 quiet, 1 normal (default), 2 verbose.
	verb := 1
	if *quiet {
		verb = 0
	}
	if *verbose {
		verb = 2
	}
	// Color only on a real terminal, unless disabled ($NO_COLOR / --no-color).
	color := !*noColor && os.Getenv("NO_COLOR") == ""
	if f, ok := stdout.(*os.File); ok {
		color = color && isTTY(f)
	} else {
		color = false
	}
	p := bakeoff.NewPalette(color)
	self, err := os.Executable()
	if err != nil {
		fmt.Fprintf(stderr, "aegis bakeoff: %v\n", err)
		return 1
	}
	// Candidate set: explicit --models, else the host-suitable models (--all or an interactive pick).
	ids, rc := resolveBakeoffModels(*models, *all, *noServe, stderr)
	if rc != 0 {
		return rc
	}
	if len(ids) == 0 {
		fmt.Fprintln(stderr, "aegis bakeoff: no models selected")
		return 2
	}
	hostLabel := *host
	if hostLabel == "" {
		hostLabel = bakeoffHostLabel()
	}
	// Auto-serve owns the endpoint (it starts a llama-server per candidate). If something is already
	// serving there, REFUSE — our servers can't take the port, so every candidate would silently measure
	// the running model. Tell the operator to free it (this was the same-model-served bug in the wild).
	if !*noServe && endpointReady(*endpoint, 2*time.Second) {
		fmt.Fprintf(stderr, "aegis bakeoff: %s is already serving a model — bakeoff needs that port to serve each candidate. Quit any running aegis / model server, or use --no-serve with an explicit --models against the live endpoint.\n", *endpoint)
		return 1
	}

	// Serve-path chatter (download progress, ctx-sizing) is silenced at quiet.
	serveOut := io.Writer(stderr)
	if verb == 0 {
		serveOut = io.Discard
	}
	suite := defaultSuite()
	var reports []bakeoff.CandidateReport
	for _, m := range ids {
		if verb >= 1 {
			fmt.Fprintf(stderr, "\n  %s %s\n", p.Cyan("▸"), p.Bold(m))
		}
		served := m
		stop := func() {}
		if *noServe {
			served = probeServedModel(*endpoint)
		} else {
			s, sm, serr := serveModelForBakeoff(m, !*noDownload, *endpoint, serveOut)
			if serr != nil {
				fmt.Fprintf(stderr, "    %s %s — %v\n", p.Red("✗ skip"), m, serr)
				continue
			}
			stop, served = s, sm
		}
		if verb >= 2 {
			fmt.Fprintf(stderr, "    %s %s\n", p.Dim("serving:"), served)
		}
		var outs []bakeoff.Outcome
		for _, task := range suite {
			o := runBakeoffCell(self, *endpoint, m, task, *timeout)
			outs = append(outs, o)
			if verb >= 1 {
				mark := p.Green("✓")
				if !o.Closed && o.FilesEdited > 0 {
					mark = p.Yellow("~")
				} else if !o.Closed {
					mark = p.Red("✗")
				}
				extra := ""
				if o.Error != "" && verb >= 2 {
					extra = p.Dim("  (" + o.Error + ")")
				}
				fmt.Fprintf(stderr, "      %s %-9s %s edited=%d  %.0fs  %d tok%s\n",
					mark, task.Name, p.Dim("·"), o.FilesEdited, float64(o.WallMs)/1000, o.OutTokens, extra)
			}
		}
		stop()
		reports = append(reports, bakeoff.Aggregate(m, served, outs))
	}
	if len(reports) == 0 {
		fmt.Fprintln(stderr, "aegis bakeoff: no candidate could be served/measured")
		return 1
	}
	cmp := bakeoff.Compare("default", hostLabel, reports)
	fmt.Fprint(stdout, cmp.Table(color))
	if err := writeComparison(*outPath, cmp); err != nil {
		fmt.Fprintf(stderr, "aegis bakeoff: write %s: %v\n", *outPath, err)
		return 1
	}
	fmt.Fprintf(stderr, "bakeoff: recorded -> %s\n", *outPath)
	return 0
}

// modelChoice is a host-suitable candidate for the interactive picker.
type modelChoice struct {
	ID          string
	TokPerSec   float64
	Interactive bool
	Present     bool
}

// suitableModels returns the origin-allowed catalog models that FIT this host's memory (from the
// profiler), largest-first, with predicted throughput + local-presence. Capacity is the bar, not
// throughput — the whole point of the bake-off is to measure whether a slow-but-fitting model is usable,
// so we don't pre-exclude it; the picker shows tok/s so the operator can decide.
func suitableModels() ([]modelChoice, error) {
	rec, err := computeRecommendation(16384)
	if err != nil {
		return nil, err
	}
	var cs []modelChoice
	for _, f := range rec.Fits {
		if !f.FitsCapacity || !f.OriginAllowed {
			continue
		}
		cs = append(cs, modelChoice{ID: f.ID, TokPerSec: f.PredictedTokPerSec, Interactive: f.FitsInteractive, Present: modelPresent(f.ID)})
	}
	return cs, nil
}

// modelPresent reports whether the catalog model's GGUF is already downloaded (size-matched).
func modelPresent(id string) bool {
	spec, err := resolveProvisionSpec(id)
	if err != nil {
		return false
	}
	fi, err := os.Stat(filepath.Join(modelDownloadDir(), spec.File))
	return err == nil && fi.Size() > 0
}

// resolveBakeoffModels picks the candidate ids: explicit --models, else the host-suitable set via --all
// or an interactive selector (TTY). --no-serve requires explicit --models (it measures the live endpoint).
func resolveBakeoffModels(models string, all, noServe bool, stderr io.Writer) ([]string, int) {
	if models != "" {
		return splitList(models), 0
	}
	if noServe {
		fmt.Fprintln(stderr, "aegis bakeoff: --no-serve needs explicit --models (it measures the already-served endpoint)")
		return nil, 2
	}
	choices, err := suitableModels()
	if err != nil {
		fmt.Fprintf(stderr, "aegis bakeoff: %v\n", err)
		return nil, 1
	}
	if len(choices) == 0 {
		fmt.Fprintln(stderr, "aegis bakeoff: no host-suitable models (see `aegis profile`)")
		return nil, 1
	}
	if all {
		ids := make([]string, len(choices))
		for i, c := range choices {
			ids[i] = c.ID
		}
		return ids, 0
	}
	if !isTTY(os.Stdin) {
		fmt.Fprintln(stderr, "aegis bakeoff: not a terminal — pass --all (every suitable model) or --models a,b")
		return nil, 2
	}
	return selectModelsInteractive(choices, os.Stdin, stderr), 0
}

// selectModelsInteractive prints the host-suitable models and reads a selection line ("1,3" or "all").
func selectModelsInteractive(choices []modelChoice, in io.Reader, out io.Writer) []string {
	fmt.Fprintln(out, "Host-suitable models (from `aegis profile`):")
	for i, c := range choices {
		speed := "slow"
		if c.Interactive {
			speed = "interactive"
		}
		state := "download"
		if c.Present {
			state = "present"
		}
		fmt.Fprintf(out, "  %d. %-22s ~%5.1f tok/s  %-11s  [%s]\n", i+1, c.ID, c.TokPerSec, speed, state)
	}
	fmt.Fprint(out, "Select models to bake off (e.g. 1,3 or 'all'): ")
	line, _ := bufio.NewReader(in).ReadString('\n')
	return parseModelSelection(line, choices)
}

// parseModelSelection turns a selection line into model ids: "all", 1-based indices, and/or bare ids.
func parseModelSelection(input string, choices []modelChoice) []string {
	input = strings.TrimSpace(strings.ToLower(input))
	if input == "" {
		return nil
	}
	if input == "all" || input == "a" {
		ids := make([]string, len(choices))
		for i, c := range choices {
			ids[i] = c.ID
		}
		return ids
	}
	seen := map[string]bool{}
	var ids []string
	for _, tok := range strings.FieldsFunc(input, func(r rune) bool { return r == ',' || r == ' ' }) {
		id := ""
		if n, err := strconv.Atoi(tok); err == nil && n >= 1 && n <= len(choices) {
			id = choices[n-1].ID
		} else {
			for _, c := range choices {
				if strings.ToLower(c.ID) == tok {
					id = c.ID
					break
				}
			}
		}
		if id != "" && !seen[id] {
			seen[id] = true
			ids = append(ids, id)
		}
	}
	return ids
}

// serveModelForBakeoff brings a candidate up on endpoint (BENCH-011), reusing the provision serve flow:
// resolve-or-download the GGUF, write a seed calibration, launch llama-server (--jinja via LaunchArgs),
// wait for readiness, and probe the served model id. Returns a stop func to tear it down before the next
// candidate. This is what makes a one-command multi-model bake-off actually swap the served model.
func serveModelForBakeoff(id string, allowDownload bool, endpoint string, out io.Writer) (func(), string, error) {
	if !allowDownload && !modelPresent(id) {
		return nil, "", fmt.Errorf("%s not present locally (omit --no-download to fetch it)", id)
	}
	gguf, ok := resolveOrDownload(id, "", out, out)
	if !ok {
		return nil, "", fmt.Errorf("could not resolve/download %s", id)
	}
	// Temp calibration on the endpoint's port, so we neither clobber the operator's persistent
	// ~/.config/aegis/calibration.json nor serve on the wrong port.
	dir, err := os.MkdirTemp("", "bakeoff-cal-")
	if err != nil {
		return nil, "", err
	}
	calPath, err := writeBakeoffCalibration(gguf, dir, endpointPort(endpoint))
	if err != nil {
		os.RemoveAll(dir)
		return nil, "", err
	}
	cmd, err := buildServeCommand(calPath)
	if err != nil {
		os.RemoveAll(dir)
		return nil, "", err
	}
	setServeProcAttr(cmd) // own process group → killServe tears down the whole tree (BENCH-012)
	cmd.Stdout, cmd.Stderr = io.Discard, io.Discard
	if err := cmd.Start(); err != nil {
		os.RemoveAll(dir)
		return nil, "", fmt.Errorf("launch server: %w", err)
	}
	stop := func() { killServe(cmd); os.RemoveAll(dir) }
	want := filepath.Base(gguf)
	deadline := time.Now().Add(180 * time.Second)
	for time.Now().Before(deadline) {
		if endpointReady(endpoint, 4*time.Second) {
			served := probeServedModel(endpoint)
			// Verify the endpoint serves THIS candidate. If a prior candidate's server (or an external
			// one) still holds the port, the served model won't match — fail loudly rather than measure
			// the wrong model (the silent bug the same-model guard had to catch after the fact).
			if served != "" && !strings.Contains(served, want) {
				stop()
				return nil, "", fmt.Errorf("endpoint %s serves %q, not %s — the previous server did not release the port", endpoint, filepath.Base(served), id)
			}
			return stop, served, nil
		}
		time.Sleep(2 * time.Second)
	}
	stop()
	return nil, "", fmt.Errorf("%s did not become ready within 180s", id)
}

// writeBakeoffCalibration writes a throwaway calibration for gguf on port into dir (never the operator's
// ~/.config), reusing the host plan + the one context resolver. Returns its path.
func writeBakeoffCalibration(gguf, dir string, port int) (string, error) {
	cal := install.Plan(install.Detect()).Calibration
	cal.Model = gguf
	cal.CtxSize = serving.ResolveCtxSize(catalogCtxSizeForGGUF(gguf))
	if port > 0 {
		cal.Port = port
	}
	b, err := json.MarshalIndent(cal, "", "  ")
	if err != nil {
		return "", err
	}
	p := filepath.Join(dir, "calibration.json")
	if err := os.WriteFile(p, b, 0o644); err != nil {
		return "", err
	}
	return p, nil
}

// endpointPort extracts the TCP port from a loopback endpoint URL (0 if absent/unparseable).
func endpointPort(endpoint string) int {
	if u, err := url.Parse(endpoint); err == nil {
		if n, err := strconv.Atoi(u.Port()); err == nil {
			return n
		}
	}
	return 0
}

// bakeoffHostLabel labels the report with the profiler's serving target, else "local".
func bakeoffHostLabel() string {
	if rec, err := computeRecommendation(16384); err == nil && rec.Profile.Target != "" {
		return rec.Profile.Target
	}
	return "local"
}

// isTTY reports whether f is a character device (an interactive terminal).
func isTTY(f *os.File) bool {
	fi, err := f.Stat()
	return err == nil && fi.Mode()&os.ModeCharDevice != 0
}

// runBakeoffCell drives one task for one model: seed a fresh git workdir, run `aegis run`, then measure
// files-edited (git), closed (verify), and turns/tokens (transcript). git is the agency ground truth —
// a file actually changed on disk means a real write/edit tool executed, no transcript interpretation.
func runBakeoffCell(bin, endpoint, model string, task bakeoff.Task, timeout time.Duration) bakeoff.Outcome {
	o := bakeoff.Outcome{Task: task.Name}
	ws, err := os.MkdirTemp("", "bakeoff-"+task.Name+"-")
	if err != nil {
		o.Error = err.Error()
		return o
	}
	defer os.RemoveAll(ws)
	if err := seedTask(ws, task); err != nil {
		o.Error = "seed: " + err.Error()
		return o
	}
	// Precondition: the task must FAIL before the run, else "closed" is meaningless.
	if runVerify(ws, task.Verify) {
		o.Error = "precondition: task passed before the run"
		return o
	}
	// Rig artifacts (cfg + transcript) live OUTSIDE the git workdir so they never pollute the
	// files-edited count — otherwise every task shows +2 phantom edits and a model that wrote NOTHING
	// would still score edited>0 (the exact failure the metric exists to catch).
	meta, err := os.MkdirTemp("", "bakeoff-meta-")
	if err != nil {
		o.Error = err.Error()
		return o
	}
	defer os.RemoveAll(meta)
	cfg := filepath.Join(meta, "cfg.json")
	_ = os.WriteFile(cfg, []byte(fmt.Sprintf(`{"endpoint":%q,"harness":"opencode","model_id":%q,"allow_egress":false}`, endpoint, model)), 0o644)
	tpath := filepath.Join(meta, "transcript.jsonl")

	t0 := time.Now()
	cmd := exec.Command(bin, "run", "--config", cfg, "--workdir", ws, "--model", model,
		"--prompt", task.Prompt, "--timeout", timeout.String(), "--out", tpath)
	cmd.Stdout, cmd.Stderr = io.Discard, io.Discard
	runErr := cmd.Run()
	o.WallMs = time.Since(t0).Milliseconds()

	o.FilesEdited = gitEditedCount(ws)
	o.Closed = runVerify(ws, task.Verify)
	o.FirstPass = o.Closed
	o.Turns, o.Tokens, o.OutTokens = transcriptStats(tpath)
	if o.Turns > 0 {
		o.ToolCalls = o.Turns // best-effort proxy until the transcript exposes tool-call validity
		if o.FilesEdited > 0 {
			o.ValidToolCalls = o.Turns
		}
	}
	if runErr != nil && o.Error == "" && !o.Closed {
		o.Error = runErr.Error()
	}
	return o
}

// seedTask writes the task's seed files and inits a git repo so files-edited is a clean `git status`.
func seedTask(ws string, task bakeoff.Task) error {
	for name, content := range task.Files {
		p := filepath.Join(ws, filepath.FromSlash(name))
		if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
			return err
		}
		if err := os.WriteFile(p, []byte(content), 0o644); err != nil {
			return err
		}
	}
	for _, argv := range [][]string{{"init", "-q"}, {"add", "-A"}, {"-c", "user.email=b@a", "-c", "user.name=b", "commit", "-q", "-m", "seed"}} {
		c := exec.Command("git", argv...)
		c.Dir = ws
		if err := c.Run(); err != nil {
			return fmt.Errorf("git %s: %w", argv[0], err)
		}
	}
	return nil
}

// gitEditedCount reports how many tracked-or-new files changed on disk since the seed commit — the agency
// ground truth (a real write/edit tool executed). 0 means the model wrote nothing (the hollow symptom).
func gitEditedCount(ws string) int {
	c := exec.Command("git", "status", "--porcelain")
	c.Dir = ws
	out, err := c.Output()
	if err != nil {
		return 0
	}
	n := 0
	for _, line := range strings.Split(strings.TrimRight(string(out), "\n"), "\n") {
		if len(line) < 4 {
			continue
		}
		// Porcelain is "XY PATH"; count only real source changes, not tool droppings (a dot-prefixed
		// path like .opencode/ that the harness may create in the workdir).
		path := strings.TrimSpace(line[3:])
		if path == "" || strings.HasPrefix(path, ".") {
			continue
		}
		n++
	}
	return n
}

// probeServedModel returns the model id the endpoint is ACTUALLY serving (GET /v1/models via the
// loopback-guarded client), or "" if it cannot be read. Recorded per candidate so Compare can detect the
// same-endpoint trap (two candidates served by one model).
func probeServedModel(endpoint string) string {
	c, err := serving.NewClient(endpoint)
	if err != nil {
		return ""
	}
	ctx, cancel := context.WithTimeout(context.Background(), 4*time.Second)
	defer cancel()
	info, err := c.ModelInfo(ctx)
	if err != nil {
		return ""
	}
	return info.ID
}

// runVerify runs the task's verify command in ws; exit 0 = closed.
func runVerify(ws string, argv []string) bool {
	if len(argv) == 0 {
		return false
	}
	c := exec.Command(argv[0], argv[1:]...)
	c.Dir = ws
	c.Env = append(os.Environ(), "GOFLAGS=-mod=mod")
	c.Stdout, c.Stderr = io.Discard, io.Discard
	return c.Run() == nil
}

// transcriptStats reads the final {"type":"result",...} line of an intent-bench transcript for the turn
// count, TOTAL tokens (input+output, for TCR/cost), and OUTPUT tokens (decode, for honest tok/s — input
// is prefill and must not be counted as throughput). Mirrors the internal/bench transcript format.
func transcriptStats(path string) (turns, total, out int) {
	b, err := os.ReadFile(path)
	if err != nil {
		return 0, 0, 0
	}
	for _, line := range strings.Split(string(b), "\n") {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		var d struct {
			Type     string `json:"type"`
			NumTurns int    `json:"num_turns"`
			Usage    struct {
				In  int `json:"input_tokens"`
				Out int `json:"output_tokens"`
			} `json:"usage"`
		}
		if json.Unmarshal([]byte(line), &d) == nil && d.Type == "result" {
			return d.NumTurns, d.Usage.In + d.Usage.Out, d.Usage.Out
		}
	}
	return 0, 0, 0
}

func writeComparison(path string, cmp bakeoff.Comparison) error {
	if dir := filepath.Dir(path); dir != "" {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			return err
		}
	}
	b, err := json.MarshalIndent(cmp, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(path, append(b, '\n'), 0o644)
}

func splitList(s string) []string {
	var out []string
	for _, p := range strings.Split(s, ",") {
		if p = strings.TrimSpace(p); p != "" {
			out = append(out, p)
		}
	}
	return out
}

func errSuffix(e string) string {
	if e == "" {
		return ""
	}
	return " (" + e + ")"
}

