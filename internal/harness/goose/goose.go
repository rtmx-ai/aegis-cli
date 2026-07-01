// Package goose is the Goose harness adapter (stub).
//
// It implements harness.Adapter by driving Goose headless over MCP. This
// scaffold provides the type and signatures; the process-driving body is filled
// in by the HARNESS requirements. The adapter uses local extensions only.
package goose

import (
	"context"
	"errors"

	"github.com/rtmx-ai/aegis-cli/internal/harness"
	"github.com/rtmx-ai/aegis-cli/internal/rtmx"
)

// Adapter drives the Goose harness.
type Adapter struct {
	// ConfigPath is the hardened, local-extensions-only goose config.
	ConfigPath string
}

// New returns a Goose Adapter using the given config path.
func New(configPath string) *Adapter {
	return &Adapter{ConfigPath: configPath}
}

// Name reports the adapter identity.
func (a *Adapter) Name() string { return "goose" }

// Drive is not yet implemented in the scaffold.
func (a *Adapter) Drive(ctx context.Context, req *rtmx.Requirement, feedback string) (harness.Diff, error) {
	return harness.Diff{RequirementID: req.ID}, errors.New("goose: Drive not implemented")
}

// Health is not yet implemented in the scaffold.
func (a *Adapter) Health(ctx context.Context) error {
	return errors.New("goose: Health not implemented")
}

// compile-time assertion that Adapter satisfies harness.Adapter.
var _ harness.Adapter = (*Adapter)(nil)
