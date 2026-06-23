package bdd

import (
	"bytes"
	"context"
	"fmt"
	"os"
	"path/filepath"
	"time"

	"github.com/cucumber/godog"

	"github.com/rtmx-ai/aegis-cli/internal/audit"
	servingharness "github.com/rtmx-ai/aegis-cli/internal/harness/serving"
	"github.com/rtmx-ai/aegis-cli/internal/loop"
	"github.com/rtmx-ai/aegis-cli/internal/metrics"
	"github.com/rtmx-ai/aegis-cli/internal/rtmx"
)

const liveFixtureHeader = "req_id,category,subcategory,requirement_text,target_value,test_module,test_function,validation_method,status,priority,phase,notes,effort_weeks,dependencies,blocks,assignee,sprint,started_date,completed_date,requirement_file,external_id\n"

// writeLiveFixture writes a temp .rtmx database under the workspace and returns
// the database path. reqs is an ordered list of (id, verifyOK).
func (w *world) writeLiveFixture(reqs [][2]any) (string, error) {
	dir := filepath.Join(w.workspace, ".rtmx")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return "", err
	}
	body := liveFixtureHeader
	verify := map[string]bool{}
	for _, r := range reqs {
		id := r[0].(string)
		verify[id] = r[1].(bool)
		body += fmt.Sprintf("%s,FEAT,X,%s,crit,pkg,T,Unit Test,OPEN,HIGH,1,,0.5,,,,,,,,\n", id, id)
	}
	db := filepath.Join(dir, "database.csv")
	if err := os.WriteFile(db, []byte(body), 0o644); err != nil {
		return "", err
	}
	w.liveClient = rtmx.NewCLIClient(db, rtmx.WithVerifyFunc(func(_ context.Context, id string) (bool, error) {
		return verify[id], nil
	}))
	return db, nil
}

func (w *world) liveBacklogCloseable(n int) error {
	reqs := make([][2]any, n)
	for i := 0; i < n; i++ {
		reqs[i] = [2]any{fmt.Sprintf("REQ-LIVE-%03d", i+1), true}
	}
	w.liveClosed = n
	_, err := w.writeLiveFixture(reqs)
	return err
}

func (w *world) liveBacklogOneCloseableOneFailing() error {
	w.liveClosed = 1
	_, err := w.writeLiveFixture([][2]any{
		{"REQ-LIVE-001", true},
		{"REQ-LIVE-002", false},
	})
	return err
}

// drainLiveBuiltin runs the loop to drain the live backlog with the real CLIClient
// and the built-in serving-backed harness against the mock model.
func (w *world) drainLiveBuiltin() error {
	w.auditBuf = &bytes.Buffer{}
	adapter := servingharness.NewWithClient(
		w.client,
		servingharness.WithWorkspace(w.workspace),
		servingharness.WithTestRunner(func(context.Context, string, *rtmx.Requirement) (bool, error) { return true, nil }),
	)
	lp, err := loop.New(w.cfg, loop.Deps{
		RTMX:    w.liveClient,
		Harness: adapter,
		Audit:   audit.New(w.auditBuf, "aegis-loop"),
		Metrics: metrics.NewCollector(),
		Now:     time.Now,
	})
	if err != nil {
		return err
	}
	w.result, w.runErr = lp.Run(context.Background(), false)
	return nil
}

func (w *world) allLiveClosed() error {
	if w.runErr != nil {
		return fmt.Errorf("live drain errored: %w", w.runErr)
	}
	if w.result.Closed != w.liveClosed {
		return fmt.Errorf("expected %d closed, got %d", w.liveClosed, w.result.Closed)
	}
	n, err := w.liveClient.Next(context.Background())
	if err != nil {
		return err
	}
	if n != nil {
		return fmt.Errorf("backlog not drained; %s still claimable", n.ID)
	}
	return nil
}

func (w *world) failingParked() error {
	if w.result.Parked < 1 {
		return fmt.Errorf("expected a parked requirement, got %d", w.result.Parked)
	}
	return nil
}

func (w *world) atLeastOneClosed() error {
	if w.result.Closed < 1 {
		return fmt.Errorf("expected at least one closed, got %d", w.result.Closed)
	}
	return nil
}

func (w *world) registerLiveSteps(sc *godog.ScenarioContext) {
	sc.Step(`^a live rtmx backlog with (\d+) closeable requirements$`, w.liveBacklogCloseable)
	sc.Step(`^a live rtmx backlog with one closeable and one failing requirement$`, w.liveBacklogOneCloseableOneFailing)
	sc.Step(`^aegis drains the backlog live with the built-in harness$`, w.drainLiveBuiltin)
	sc.Step(`^all live requirements are closed$`, w.allLiveClosed)
	sc.Step(`^the failing requirement is parked$`, w.failingParked)
	sc.Step(`^at least one requirement is closed$`, w.atLeastOneClosed)
}
