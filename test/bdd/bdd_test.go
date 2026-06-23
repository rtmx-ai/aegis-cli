// Package bdd runs the Gherkin feature corpus (../../features) as executable
// end-to-end tests via godog. Each scenario drives the REAL components — the
// rtmx-driven control loop, the loopback serving client against a mock model
// server, the append-only audit log, and the installer planner — so the suite
// incrementally exposes aegis-cli's user-visible features end to end.
//
// godog is a TEST-only dependency (vendored). The shipped binary stays
// std-lib-only; nothing here is imported by cmd/ or non-test internal/ code.
package bdd

import (
	"bytes"
	"context"
	"fmt"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/cucumber/godog"

	"github.com/rtmx-ai/aegis-cli/internal/audit"
	"github.com/rtmx-ai/aegis-cli/internal/config"
	"github.com/rtmx-ai/aegis-cli/internal/harness"
	"github.com/rtmx-ai/aegis-cli/internal/install"
	"github.com/rtmx-ai/aegis-cli/internal/loop"
	"github.com/rtmx-ai/aegis-cli/internal/metrics"
	"github.com/rtmx-ai/aegis-cli/internal/mockmodel"
	"github.com/rtmx-ai/aegis-cli/internal/rtmx"
	"github.com/rtmx-ai/aegis-cli/internal/serving"
)

// servingAdapter is a harness.Adapter that drives a requirement by asking the
// local model (over the loopback serving client) for a patch. It is the test's
// stand-in for a production serving-backed harness, exercising the real
// serving→loop→rtmx→audit thread end to end.
type servingAdapter struct{ client *serving.Client }

func (a *servingAdapter) Name() string { return "serving-mock" }

func (a *servingAdapter) Health(ctx context.Context) error { return nil }

func (a *servingAdapter) Drive(ctx context.Context, req *rtmx.Requirement) (harness.Diff, error) {
	resp, err := a.client.ChatCompletion(ctx, serving.ChatRequest{
		Model:    "test-model",
		Messages: []serving.Message{{Role: "user", Content: "implement " + req.ID}},
	})
	if err != nil {
		return harness.Diff{RequirementID: req.ID}, err
	}
	content := ""
	if len(resp.Choices) > 0 {
		content = resp.Choices[0].Message.Content
	}
	return harness.Diff{RequirementID: req.ID, Patch: content, Turns: 1, ToolCalls: 1, ValidToolCalls: 1, Tokens: len(content)}, nil
}

// world holds per-scenario state.
type world struct {
	mock      *mockmodel.Server
	client    *serving.Client
	clientErr error

	fake *rtmx.Fake
	cfg  config.Config

	result loop.Result
	runErr error

	auditBuf   *bytes.Buffer
	completion string

	caps install.HostCaps
	plan install.InstallPlan

	workspace string
	editedRel string
}

func (w *world) reset() {
	if w.mock != nil {
		w.mock.Close()
	}
	*w = world{cfg: config.Default()}
}

// --- Given ------------------------------------------------------------------

func (w *world) aMockModelEndpointOnLoopback() error {
	w.mock = mockmodel.New(mockmodel.Options{
		Handler: func(mockmodel.RecordedRequest) (mockmodel.Response, bool) {
			return mockmodel.Response{Content: "--- a/impl\n+++ b/impl\n+done\n"}, true
		},
		ModelID:     "test-model",
		ModelDigest: "sha256:deadbeef",
	})
	c, err := serving.NewClient(w.mock.URL())
	if err != nil {
		return err
	}
	w.client = c
	w.cfg.Endpoint = w.mock.URL()
	return nil
}

func (w *world) backlog(n int, verify bool) {
	reqs := make([]*rtmx.Requirement, n)
	for i := 0; i < n; i++ {
		reqs[i] = &rtmx.Requirement{ID: fmt.Sprintf("FEAT-DEMO-%d", i+1), Status: rtmx.StatusOpen}
	}
	w.fake = rtmx.NewFake(reqs...)
	for _, r := range reqs {
		w.fake.VerifyResult[r.ID] = verify
	}
}

func (w *world) oneReqVerifiesOK() error     { w.backlog(1, true); return nil }
func (w *world) oneReqAlwaysFails() error    { w.backlog(1, false); return nil }
func (w *world) nReqsAlwaysFail(n int) error { w.backlog(n, false); return nil }
func (w *world) nReqsVerifyOK(n int) error   { w.backlog(n, true); return nil }

func (w *world) retryBudget(attempts int) error { w.cfg.Retries = attempts - 1; return nil }
func (w *world) breakerAfter(n int) error       { w.cfg.BreakAfter = n; return nil }
func (w *world) runBudget(n int) error          { w.cfg.Budget.MaxRequirements = n; return nil }

func (w *world) hostWithMemory(os string, ramGiB int) error {
	accel := install.AccelNone
	arch := "amd64"
	if os == "darwin" {
		accel = install.AccelMetal
		arch = "arm64"
	}
	w.caps = install.HostCaps{
		OS: os, Arch: arch, LogicalCPU: 16, PhysicalCPU: 8,
		TotalRAMBytes: uint64(ramGiB) << 30, Accel: accel,
	}
	return nil
}

// --- When -------------------------------------------------------------------

func (w *world) runLoop(once bool) error {
	w.auditBuf = &bytes.Buffer{}
	adapter := &servingAdapter{client: w.client}
	lp, err := loop.New(w.cfg, loop.Deps{
		RTMX:    w.fake,
		Harness: adapter,
		Audit:   audit.New(w.auditBuf, "aegis-loop"),
		Metrics: metrics.NewCollector(),
		Now:     time.Now,
	})
	if err != nil {
		return err
	}
	w.result, w.runErr = lp.Run(context.Background(), once)
	return nil
}

func (w *world) runsOneIteration() error { return w.runLoop(true) }
func (w *world) drainsBacklog() error    { return w.runLoop(false) }

func (w *world) requestsChatCompletion() error {
	resp, err := w.client.ChatCompletion(context.Background(), serving.ChatRequest{
		Model:    "test-model",
		Messages: []serving.Message{{Role: "user", Content: "hello"}},
	})
	if err != nil {
		return err
	}
	if len(resp.Choices) > 0 {
		w.completion = resp.Choices[0].Message.Content
	}
	return nil
}

func (w *world) clientConstructedNonLoopback() error {
	_, w.clientErr = serving.NewClient("http://8.8.8.8:8080")
	return nil
}

func (w *world) initPlansBootstrap() error { w.plan = install.Plan(w.caps); return nil }

// --- Then -------------------------------------------------------------------

func (w *world) requirementClosed() error {
	if w.runErr != nil {
		return fmt.Errorf("run errored: %w", w.runErr)
	}
	if w.result.Closed != 1 {
		return fmt.Errorf("expected 1 closed, got %d", w.result.Closed)
	}
	return nil
}

func (w *world) auditRecordsClaimAndVerify() error {
	s := w.auditBuf.String()
	if !strings.Contains(s, `"action":"claim"`) {
		return fmt.Errorf("audit missing claim:\n%s", s)
	}
	if !strings.Contains(s, `"action":"verify"`) {
		return fmt.Errorf("audit missing verify:\n%s", s)
	}
	return nil
}

func (w *world) endpointReceivedCompletion() error {
	for _, r := range w.mock.Requests() {
		if strings.Contains(r.Path, "/chat/completions") {
			return nil
		}
	}
	return fmt.Errorf("model endpoint received no completion request (%d requests)", len(w.mock.Requests()))
}

func (w *world) requirementParked() error {
	if w.result.Parked < 1 {
		return fmt.Errorf("expected a parked requirement, got %d", w.result.Parked)
	}
	return nil
}

func (w *world) breakerTrips() error {
	if !w.result.BreakerTripped {
		return fmt.Errorf("expected the circuit breaker to trip")
	}
	return nil
}

func (w *world) runStopsOnBudget() error {
	if !w.result.BudgetExhausted {
		return fmt.Errorf("expected the run to stop on the budget")
	}
	return nil
}

func (w *world) exactlyNClosed(n int) error {
	if w.result.Closed != n {
		return fmt.Errorf("expected %d closed, got %d", n, w.result.Closed)
	}
	return nil
}

func (w *world) completionReturned() error {
	if w.completion == "" {
		return fmt.Errorf("expected completion content, got empty")
	}
	return nil
}

func (w *world) constructionRefused() error {
	if w.clientErr == nil {
		return fmt.Errorf("expected non-loopback construction to be refused")
	}
	return nil
}

func (w *world) plannedTarget(target string) error {
	if string(w.plan.Target) != target {
		return fmt.Errorf("expected target %q, got %q", target, w.plan.Target)
	}
	return nil
}

// InitializeScenario binds the Gherkin steps to the world.
func InitializeScenario(sc *godog.ScenarioContext) {
	w := &world{cfg: config.Default()}
	sc.Before(func(ctx context.Context, _ *godog.Scenario) (context.Context, error) {
		w.reset()
		return ctx, nil
	})
	sc.After(func(ctx context.Context, _ *godog.Scenario, _ error) (context.Context, error) {
		if w.mock != nil {
			w.mock.Close()
		}
		if w.workspace != "" {
			_ = os.RemoveAll(w.workspace)
		}
		return ctx, nil
	})
	w.registerHarnessSteps(sc)

	sc.Step(`^a mock model endpoint on loopback$`, w.aMockModelEndpointOnLoopback)
	sc.Step(`^a backlog with one requirement that will verify successfully$`, w.oneReqVerifiesOK)
	sc.Step(`^a backlog with one requirement that always fails verification$`, w.oneReqAlwaysFails)
	sc.Step(`^a backlog with (\d+) requirements that always fail verification$`, w.nReqsAlwaysFail)
	sc.Step(`^a backlog with (\d+) requirements that will verify successfully$`, w.nReqsVerifyOK)
	sc.Step(`^a retry budget of (\d+) attempts$`, w.retryBudget)
	sc.Step(`^a circuit breaker after (\d+) consecutive failures$`, w.breakerAfter)
	sc.Step(`^a run budget of (\d+) requirement$`, w.runBudget)
	sc.Step(`^a host running "([^"]*)" with (\d+) GiB of memory$`, w.hostWithMemory)

	sc.Step(`^aegis runs one iteration$`, w.runsOneIteration)
	sc.Step(`^aegis drains the backlog$`, w.drainsBacklog)
	sc.Step(`^the client requests a chat completion$`, w.requestsChatCompletion)
	sc.Step(`^the client is constructed for a non-loopback endpoint$`, w.clientConstructedNonLoopback)
	sc.Step(`^aegis init plans the bootstrap$`, w.initPlansBootstrap)

	sc.Step(`^the requirement is closed by verify$`, w.requirementClosed)
	sc.Step(`^the audit log records a claim and a verify$`, w.auditRecordsClaimAndVerify)
	sc.Step(`^the model endpoint received a completion request$`, w.endpointReceivedCompletion)
	sc.Step(`^the requirement is parked$`, w.requirementParked)
	sc.Step(`^the circuit breaker trips$`, w.breakerTrips)
	sc.Step(`^the run stops on the budget$`, w.runStopsOnBudget)
	sc.Step(`^exactly (\d+) requirement is closed$`, w.exactlyNClosed)
	sc.Step(`^the completion content is returned$`, w.completionReturned)
	sc.Step(`^client construction is refused$`, w.constructionRefused)
	sc.Step(`^the planned target is "([^"]*)"$`, w.plannedTarget)
}

// TestFeatures runs the Gherkin corpus as Go tests. Each scenario is a FEAT-*
// requirement closed by rtmx via this test function.
func TestFeatures(t *testing.T) {
	suite := godog.TestSuite{
		Name:                "aegis-bdd",
		ScenarioInitializer: InitializeScenario,
		Options: &godog.Options{
			Format:   "pretty",
			Paths:    []string{"../../features"},
			TestingT: t,
			Strict:   true,
		},
	}
	if suite.Run() != 0 {
		t.Fatal("there were failed BDD scenarios")
	}
}
