package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"

	"github.com/rtmx-ai/aegis-cli/internal/bakeoff"
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
	models := fs.String("models", "", "comma-separated model ids to compare (each must be servable/served)")
	endpoint := fs.String("endpoint", "http://127.0.0.1:8080", "OpenAI-compatible loopback endpoint the models are served on")
	timeout := fs.Duration("timeout", 300*time.Second, "per-task wall-clock budget")
	outPath := fs.String("out", "eval/bakeoff/comparison.json", "where to write the comparison JSON")
	host := fs.String("host", "", "host label for the report (default: from `aegis profile` target)")
	if err := fs.Parse(args); err != nil {
		return 2
	}
	ms := splitList(*models)
	if len(ms) < 2 {
		fmt.Fprintln(stderr, "aegis bakeoff: need >=2 --models to compare (e.g. --models gemma-4-26b-a4b,devstral-small-2507)")
		return 2
	}
	self, err := os.Executable()
	if err != nil {
		fmt.Fprintf(stderr, "aegis bakeoff: %v\n", err)
		return 1
	}
	hostLabel := *host
	if hostLabel == "" {
		hostLabel = servingModelID() // best-effort; the profile target is nicer but this avoids a probe
		if hostLabel == "" {
			hostLabel = "local"
		}
	}
	suite := defaultSuite()
	var reports []bakeoff.CandidateReport
	for _, m := range ms {
		fmt.Fprintf(stderr, "bakeoff: %s\n", m)
		var outs []bakeoff.Outcome
		for _, task := range suite {
			o := runBakeoffCell(self, *endpoint, m, task, *timeout)
			outs = append(outs, o)
			status := "----"
			if o.Closed {
				status = "PASS"
			} else if o.FilesEdited > 0 {
				status = "edit" // wrote code but did not pass verify (capable, imperfect)
			}
			fmt.Fprintf(stderr, "  %-8s %s  edited=%d wall=%.0fs tokens=%d%s\n",
				task.Name, status, o.FilesEdited, float64(o.WallMs)/1000, o.Tokens, errSuffix(o.Error))
		}
		reports = append(reports, bakeoff.Aggregate(m, outs))
	}
	cmp := bakeoff.Compare("default", hostLabel, reports)
	fmt.Fprint(stdout, cmp.Table())
	if err := writeComparison(*outPath, cmp); err != nil {
		fmt.Fprintf(stderr, "aegis bakeoff: write %s: %v\n", *outPath, err)
		return 1
	}
	fmt.Fprintf(stderr, "bakeoff: recorded -> %s\n", *outPath)
	return 0
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
	cfg := filepath.Join(ws, "cfg.json")
	_ = os.WriteFile(cfg, []byte(fmt.Sprintf(`{"endpoint":%q,"harness":"opencode","model_id":%q,"allow_egress":false}`, endpoint, model)), 0o644)
	tpath := filepath.Join(ws, "transcript.jsonl")

	t0 := time.Now()
	cmd := exec.Command(bin, "run", "--config", cfg, "--workdir", ws, "--model", model,
		"--prompt", task.Prompt, "--timeout", timeout.String(), "--out", tpath)
	cmd.Stdout, cmd.Stderr = io.Discard, io.Discard
	runErr := cmd.Run()
	o.WallMs = time.Since(t0).Milliseconds()

	o.FilesEdited = gitEditedCount(ws)
	o.Closed = runVerify(ws, task.Verify)
	o.FirstPass = o.Closed
	o.Turns, o.Tokens = transcriptTurnsTokens(tpath)
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
		if strings.TrimSpace(line) != "" {
			n++
		}
	}
	return n
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

// transcriptTurnsTokens reads the final {"type":"result",...} line of an intent-bench transcript for the
// turn count + total tokens (mirrors internal/bench transcript format; std parse, no interpretation).
func transcriptTurnsTokens(path string) (turns, tokens int) {
	b, err := os.ReadFile(path)
	if err != nil {
		return 0, 0
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
			return d.NumTurns, d.Usage.In + d.Usage.Out
		}
	}
	return 0, 0
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
