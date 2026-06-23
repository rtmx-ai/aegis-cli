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
	"github.com/rtmx-ai/aegis-cli/internal/harness"
	servingharness "github.com/rtmx-ai/aegis-cli/internal/harness/serving"
	"github.com/rtmx-ai/aegis-cli/internal/loop"
	"github.com/rtmx-ai/aegis-cli/internal/metrics"
	"github.com/rtmx-ai/aegis-cli/internal/mockmodel"
	"github.com/rtmx-ai/aegis-cli/internal/rtmx"
	"github.com/rtmx-ai/aegis-cli/internal/serving"
)

// editBlock renders a parseable ```file <path> ... ``` block for the mock model.
func editBlock(path, body string) string {
	return "```file " + path + "\n" + body + "\n```\n"
}

// setupMock points the world's serving client at a mock with the given scripted
// responses (consumed in order).
func (w *world) setupMock(resps ...mockmodel.Response) error {
	w.mock = mockmodel.New(mockmodel.Options{Responses: resps})
	c, err := serving.NewClient(w.mock.URL())
	if err != nil {
		return err
	}
	w.client = c
	return nil
}

func (w *world) aWorkspace() error {
	dir, err := os.MkdirTemp("", "aegis-bdd-ws")
	if err != nil {
		return err
	}
	w.workspace = dir
	return nil
}

func (w *world) mockEmitsValidEdit() error {
	w.editedRel = "impl.go"
	return w.setupMock(mockmodel.Response{Content: editBlock(w.editedRel, "package impl")})
}

func (w *world) mockMalformedThenValid() error {
	w.editedRel = "impl.go"
	return w.setupMock(
		mockmodel.Response{Content: "sorry, here is some prose with no file block"},
		mockmodel.Response{Content: editBlock(w.editedRel, "package impl")},
	)
}

func (w *world) mockEmitsOutOfWorkspaceEdit() error {
	return w.setupMock(mockmodel.Response{Content: editBlock("../escape.txt", "pwned")})
}

// runOneIterationBuiltin drives the loop for one iteration using the real
// built-in serving-backed harness against the mock model + workspace.
func (w *world) runOneIterationBuiltin() error {
	w.auditBuf = &bytes.Buffer{}
	adapter := servingharness.NewWithClient(
		w.client,
		servingharness.WithWorkspace(w.workspace),
		servingharness.WithTestRunner(func(context.Context, string, *rtmx.Requirement) (bool, error) { return true, nil }),
	)
	return w.runLoopWith(adapter, true)
}

// runLoopWith runs the loop with a specific harness adapter.
func (w *world) runLoopWith(adapter harness.Adapter, once bool) error {
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

func (w *world) editedFileExists() error {
	if _, err := os.Stat(filepath.Join(w.workspace, w.editedRel)); err != nil {
		return fmt.Errorf("expected edited file %q in workspace: %w", w.editedRel, err)
	}
	return nil
}

func (w *world) noFileOutsideWorkspace() error {
	escaped := filepath.Join(filepath.Dir(w.workspace), "escape.txt")
	if _, err := os.Stat(escaped); !os.IsNotExist(err) {
		return fmt.Errorf("a file was written outside the workspace at %q", escaped)
	}
	return nil
}

// registerHarnessSteps binds the built-in-harness scenario steps.
func (w *world) registerHarnessSteps(sc *godog.ScenarioContext) {
	sc.Step(`^a workspace$`, w.aWorkspace)
	sc.Step(`^a mock model that emits a valid file edit$`, w.mockEmitsValidEdit)
	sc.Step(`^a mock model that first emits malformed output then a valid edit$`, w.mockMalformedThenValid)
	sc.Step(`^a mock model that emits an out-of-workspace edit$`, w.mockEmitsOutOfWorkspaceEdit)
	sc.Step(`^aegis runs one iteration with the built-in harness$`, w.runOneIterationBuiltin)
	sc.Step(`^the edited file exists in the workspace$`, w.editedFileExists)
	sc.Step(`^no file is written outside the workspace$`, w.noFileOutsideWorkspace)
}
