// Package opencode resolves and launches the bundled OpenCode TUI — aegis's
// centerpiece agentic-coding experience (CLAUDE.md §1). aegis bundles and
// launches OpenCode; it does not fork or rebuild it. The launch is driven by the
// air-gap-hardened config (deploy/opencode/opencode.json): offline, telemetry
// off, model on loopback, rtmx registered as the MCP intent layer.
package opencode

import (
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"

	"github.com/rtmx-ai/aegis-cli/internal/config"
)

// DefaultConfigPath is the hardened OpenCode config aegis launches with.
const DefaultConfigPath = "deploy/opencode/opencode.json"

// StagedRelPath is where `scripts/build-opencode.sh` writes the self-built,
// air-gap-hardened OpenCode binary (OC-002). aegis resolves it there when it is
// not on PATH or alongside the aegis binary.
const StagedRelPath = "deploy/opencode/bin/opencode"

// ErrMissing is returned when the OpenCode binary cannot be found.
var ErrMissing = errors.New("opencode: binary not found")

// MissingGuidance tells the operator how to stage/bundle OpenCode.
const MissingGuidance = `aegis: OpenCode (the centerpiece TUI) is not installed or bundled.
Stage the opencode binary on PATH or alongside the aegis binary:
  - bundled releases ship it next to aegis (see make release)
  - or install OpenCode on the connected build host before transfer to the enclave
See docs/requirements/opencode-tui.md.`

// IsMissing reports whether err indicates a missing OpenCode binary.
func IsMissing(err error) bool { return errors.Is(err, ErrMissing) }

func isExecutable(path string) bool {
	fi, err := os.Stat(path)
	return err == nil && !fi.IsDir() && fi.Mode().Perm()&0o111 != 0
}

// ResolveBinary finds the OpenCode binary: an explicit path, then PATH, then
// alongside the running aegis executable (the bundled-distribution location).
func ResolveBinary(explicit string) (string, error) {
	if explicit != "" {
		if isExecutable(explicit) {
			return explicit, nil
		}
		return "", fmt.Errorf("%w: %s is not executable", ErrMissing, explicit)
	}
	if p, err := exec.LookPath("opencode"); err == nil {
		return p, nil
	}
	// Candidate locations, in order: alongside the aegis binary (bundled release),
	// alongside it under the staged path, and the staged path relative to cwd.
	var cands []string
	if self, err := os.Executable(); err == nil {
		dir := filepath.Dir(self)
		cands = append(cands, filepath.Join(dir, "opencode"), filepath.Join(dir, StagedRelPath))
	}
	cands = append(cands, StagedRelPath)
	for _, c := range cands {
		if isExecutable(c) {
			return c, nil
		}
	}
	return "", ErrMissing
}

// Command builds the exec.Cmd that launches the OpenCode TUI under the hardened
// config + loopback model, inheriting the terminal. It does not run it. The
// exact OpenCode flag/env contract is validated against a staged OpenCode build;
// aegis controls the hardened config, the loopback endpoint, and the air-gap
// env markers asserted here.
func Command(cfg config.Config, bin, configPath string) *exec.Cmd {
	if configPath == "" {
		configPath = DefaultConfigPath
	}
	cmd := exec.Command(bin)
	cmd.Env = append(os.Environ(),
		"OPENCODE_CONFIG="+configPath,         // hardened, offline config (rtmx MCP + loopback model)
		"OPENCODE_AUTOUPDATE=0",               // air-gap: never self-update
		"OPENCODE_TELEMETRY=0",                // air-gap: no telemetry
		"OPENCODE_DISABLE_SHARE=1",            // air-gap: no share/upload
		"OPENAI_BASE_URL="+cfg.Endpoint+"/v1", // local loopback model
		"OPENAI_API_KEY=not-needed-loopback",
	)
	cmd.Stdin, cmd.Stdout, cmd.Stderr = os.Stdin, os.Stdout, os.Stderr
	return cmd
}

// Launch resolves OpenCode and runs its TUI under the hardened config. It returns
// ErrMissing (caller prints MissingGuidance) when the binary is absent.
func Launch(cfg config.Config, explicitBin, configPath string) error {
	bin, err := ResolveBinary(explicitBin)
	if err != nil {
		return err
	}
	return Command(cfg, bin, configPath).Run()
}
