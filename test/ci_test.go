// Package offline — CI-parity and air-gap-gate acceptance tests.
//
// These are System Tests for repo-level artifacts (the Makefile, the GitHub
// Actions workflow, the git hooks, and the egress gate). They are the
// closed-loop verification behind REQ-CI-001..005 and REQ-GUARD-001: each test
// asserts that an artifact actually satisfies its requirement's acceptance
// criterion, so `rtmx verify` can close the requirement from a passing run.
package offline

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// readRepoFile reads a path relative to the module root or fails the test.
func readRepoFile(t *testing.T, rel string) string {
	t.Helper()
	data, err := os.ReadFile(filepath.Join(repoRoot(t), rel))
	if err != nil {
		t.Fatalf("read %s: %v", rel, err)
	}
	return string(data)
}

// makeTargetBody returns the recipe lines of a `name:` target from a Makefile
// body — specifically the prerequisites listed on the target line, which is
// what we assert the pipeline ordering against.
func makeTargetPrereqs(t *testing.T, makefile, name string) string {
	t.Helper()
	for _, line := range strings.Split(makefile, "\n") {
		if strings.HasPrefix(line, name+":") {
			return strings.TrimSpace(strings.TrimPrefix(line, name+":"))
		}
	}
	t.Fatalf("make target %q not found", name)
	return ""
}

// TestCIWorkflowRunsMakeCI models REQ-CI-001: a single GitHub Actions workflow
// invokes `make ci`, and that target chains every pipeline stage so the build
// is gated end-to-end.
func TestCIWorkflowRunsMakeCI(t *testing.T) {
	wf := readRepoFile(t, ".github/workflows/ci.yml")
	if !strings.Contains(wf, "run: make ci") {
		t.Error("CI workflow must invoke `make ci` as its build step")
	}
	// The pipeline is defined once, in the Makefile `ci` target, and must run
	// build -> unit -> airgap (EGRESS=0) -> health (TRACE) -> metrics (ACR).
	mk := readRepoFile(t, "Makefile")
	prereqs := makeTargetPrereqs(t, mk, "ci")
	for _, stage := range []string{"build", "test", "airgap", "health", "metrics"} {
		if !strings.Contains(prereqs, stage) {
			t.Errorf("make ci must run the %q stage (prereqs: %q)", stage, prereqs)
		}
	}
}

// TestCIParitySingleSourceOfTruth models REQ-CI-002: the workflow does not
// duplicate any pipeline step — it calls the same `make ci` the pre-push hook
// calls. Parity is structural: one target, every actor.
func TestCIParitySingleSourceOfTruth(t *testing.T) {
	wf := readRepoFile(t, ".github/workflows/ci.yml")
	// The workflow must not re-implement pipeline steps outside `make ci`.
	for _, dup := range []string{"go build", "go test", "go vet", "gofmt"} {
		if strings.Contains(wf, "run: "+dup) {
			t.Errorf("workflow duplicates pipeline step %q outside `make ci` — parity must be structural", dup)
		}
	}
	mk := readRepoFile(t, "Makefile")
	if !strings.Contains(mk, "\nci:") {
		t.Error("Makefile must define the single-source-of-truth `ci` target")
	}
	// The pre-push hook the installer writes must call the identical target.
	if !strings.Contains(readRepoFile(t, "scripts/install-hooks.sh"), `write_hook "pre-push" "ci"`) {
		t.Error("pre-push hook must call `make ci` (same target as CI)")
	}
}

// installHooks runs the installer from the repo root and returns combined output.
func installHooks(t *testing.T) {
	t.Helper()
	cmd := exec.Command("bash", "scripts/install-hooks.sh")
	cmd.Dir = repoRoot(t)
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("install-hooks.sh failed: %v\n%s", err, out)
	}
}

// TestPreCommitHookRunsCIFast models REQ-CI-003: the installed pre-commit hook
// runs `make ci-fast`, and that subset covers fmt-check, vet, build, test, health.
func TestPreCommitHookRunsCIFast(t *testing.T) {
	installHooks(t)
	hook := readRepoFile(t, ".git/hooks/pre-commit")
	if !strings.Contains(hook, "make ci-fast") {
		t.Error("pre-commit hook must run `make ci-fast`")
	}
	prereqs := makeTargetPrereqs(t, readRepoFile(t, "Makefile"), "ci-fast")
	for _, stage := range []string{"fmt-check", "vet", "build", "test", "health"} {
		if !strings.Contains(prereqs, stage) {
			t.Errorf("make ci-fast must run the %q stage (prereqs: %q)", stage, prereqs)
		}
	}
}

// TestPrePushHookRunsMakeCI models REQ-CI-004: the installed pre-push hook runs
// the full `make ci` — no push reaches CI without passing the identical gate.
func TestPrePushHookRunsMakeCI(t *testing.T) {
	installHooks(t)
	hook := readRepoFile(t, ".git/hooks/pre-push")
	if !strings.Contains(hook, "make ci") || strings.Contains(hook, "make ci-fast") {
		t.Errorf("pre-push hook must run the full `make ci`, got:\n%s", hook)
	}
}

// TestInstallHooksIdempotent models REQ-CI-005: re-running the installer is a
// no-op — the managed hooks are byte-identical across runs (no growth, no
// duplication) and remain executable.
func TestInstallHooksIdempotent(t *testing.T) {
	installHooks(t)
	first := map[string]string{
		"pre-commit": readRepoFile(t, ".git/hooks/pre-commit"),
		"pre-push":   readRepoFile(t, ".git/hooks/pre-push"),
	}
	installHooks(t) // second run must not change anything
	for name, want := range first {
		got := readRepoFile(t, ".git/hooks/"+name)
		if got != want {
			t.Errorf("hook %q changed on re-install — installer is not idempotent", name)
		}
		info, err := os.Stat(filepath.Join(repoRoot(t), ".git/hooks", name))
		if err != nil {
			t.Fatalf("stat %s: %v", name, err)
		}
		if info.Mode().Perm()&0o100 == 0 {
			t.Errorf("hook %q is not executable", name)
		}
	}
}

// runAirgap runs the egress gate around a command and returns its exit code.
func runAirgap(t *testing.T, args ...string) int {
	t.Helper()
	full := append([]string{"scripts/verify-airgap.sh", "--"}, args...)
	cmd := exec.Command("bash", full...)
	cmd.Dir = repoRoot(t)
	err := cmd.Run()
	if err == nil {
		return 0
	}
	if ee, ok := err.(*exec.ExitError); ok {
		return ee.ExitCode()
	}
	t.Fatalf("running airgap gate: %v", err)
	return -1
}

// netnsAvailable reports whether unprivileged network-namespace isolation works
// here (the strongest, fail-closed gate branch — always available in CI).
func netnsAvailable() bool {
	return exec.Command("unshare", "-rn", "true").Run() == nil
}

// TestAirgapGateFailsClosed models REQ-GUARD-001: the egress gate fails the
// build when a run cannot complete cleanly, rejects malformed invocation, and
// passes a loopback-only run. Where netns isolation is available (CI), it
// additionally proves a real non-loopback egress attempt is blocked.
func TestAirgapGateFailsClosed(t *testing.T) {
	// Malformed invocation (no `--`) must be a usage error, not a silent pass.
	if rc := runAirgapRaw(t); rc == 0 {
		t.Error("egress gate must reject invocation without a `-- <cmd>`")
	}
	// A clean loopback-only run passes the gate.
	if rc := runAirgap(t, "true"); rc != 0 {
		t.Errorf("egress gate should PASS a loopback-only command, got exit %d", rc)
	}
	// A run that fails must fail the gate (in netns, an egress attempt manifests
	// exactly this way: no route off-box -> command fails -> build fails).
	if rc := runAirgap(t, "false"); rc == 0 {
		t.Error("egress gate must propagate a failing run as a build failure")
	}
	// Strongest assertion, available in CI: a genuine non-loopback connection
	// attempt is blocked inside the egress-less namespace and fails the gate.
	if netnsAvailable() {
		// 10.255.255.1 is non-routable inside the isolated netns (loopback only).
		rc := runAirgap(t, "bash", "-c", "exec 3<>/dev/tcp/10.255.255.1/80")
		if rc == 0 {
			t.Error("egress gate must FAIL a real non-loopback egress attempt under netns")
		}
	} else {
		t.Log("netns unavailable on this host (userns restricted); live-egress sub-check runs in CI. Gate fell back to ss/dev-host branch for the loopback-pass assertion.")
	}
}

// TestOperatorDocsPresent models REQ-DOCS-001: operator docs are present and cover their
// required topics — the README's install + first-run entry points, a runbook, and an
// air-gap setup guide.
func TestOperatorDocsPresent(t *testing.T) {
	// README must give install + first-run entry points.
	readme := readRepoFile(t, "README.md")
	for _, topic := range []string{"brew install rtmx-ai/tap/aegis", "make build", "verify-env"} {
		if !strings.Contains(readme, topic) {
			t.Errorf("README must cover the install/usage entry point %q", topic)
		}
	}
	// Runbook must cover operating the loop and closed-loop verification.
	runbook := readRepoFile(t, "docs/runbook.md")
	for _, topic := range []string{"aegis run", "rtmx verify", "verify-env", "make ci"} {
		if !strings.Contains(runbook, topic) {
			t.Errorf("runbook must cover %q", topic)
		}
	}
	// Air-gap setup guide must cover the closed-environment procedure.
	airgap := readRepoFile(t, "docs/airgap-setup.md")
	for _, topic := range []string{"verify-airgap.sh", "EGRESS", "firewall", "calibrat"} {
		if !strings.Contains(airgap, topic) {
			t.Errorf("air-gap setup guide must cover %q", topic)
		}
	}
}

// TestReadmeBadgesPresent models REQ-DOCS-002: the README carries the five live
// status badges, the badge generator emits valid shields "endpoint" JSON for the
// two computed badges, and CI regenerates them each run via `make badges`.
func TestReadmeBadgesPresent(t *testing.T) {
	readme := readRepoFile(t, "README.md")
	badges := map[string]string{
		"CI status": "actions/workflows/ci.yml/badge.svg",
		"coverage":  "raw.githubusercontent.com/rtmx-ai/aegis-cli/badges/coverage.json",
		"version":   "raw.githubusercontent.com/rtmx-ai/aegis-cli/badges/version.json",
		"Go grade":  "goreportcard.com/badge/github.com/rtmx-ai/aegis-cli",
		"license":   "img.shields.io/github/license/rtmx-ai/aegis-cli",
	}
	for name, frag := range badges {
		if !strings.Contains(readme, frag) {
			t.Errorf("README missing the %s badge (expected URL fragment %q)", name, frag)
		}
	}
	// The generator must emit a valid shields endpoint payload (schemaVersion +
	// label + message) for both computed badges. Assert on the script source so
	// the test never re-invokes coverage (which would recurse into `go test`).
	gen := readRepoFile(t, "scripts/gen-badges.sh")
	for _, key := range []string{`"schemaVersion":1`, "coverage.json", "version.json"} {
		if !strings.Contains(gen, key) {
			t.Errorf("gen-badges.sh must emit %q", key)
		}
	}
	// CI must regenerate the badge data each run via the single-source target.
	wf := readRepoFile(t, ".github/workflows/ci.yml")
	if !strings.Contains(wf, "make badges") || !strings.Contains(wf, "badges:") {
		t.Error("CI workflow must have a badges job that runs `make badges`")
	}
}

// TestApacheLicensePresent models REQ-DOCS-003: the project is licensed
// Apache-2.0 with LICENSE + NOTICE present and the README license badge links it.
func TestApacheLicensePresent(t *testing.T) {
	lic := readRepoFile(t, "LICENSE")
	for _, want := range []string{"Apache License", "Version 2.0", "ioTACTICAL LLC"} {
		if !strings.Contains(lic, want) {
			t.Errorf("LICENSE must contain %q", want)
		}
	}
	if strings.Contains(lic, "[name of copyright owner]") {
		t.Error("LICENSE still has the unfilled copyright placeholder")
	}
	notice := readRepoFile(t, "NOTICE")
	if !strings.Contains(notice, "ioTACTICAL LLC") {
		t.Error("NOTICE must assert the ioTACTICAL LLC copyright")
	}
	if !strings.Contains(readRepoFile(t, "README.md"), "](LICENSE)") {
		t.Error("README license badge must link to LICENSE")
	}
}

// TestCIHardeningGates models REQ-CI-006: the `ci` pipeline runs the hardening
// gates (lint, race, cover-gate, vuln) on top of the original stages, and CI
// installs the tools that enforce lint + vuln.
func TestCIHardeningGates(t *testing.T) {
	prereqs := makeTargetPrereqs(t, readRepoFile(t, "Makefile"), "ci")
	for _, gate := range []string{"lint", "race", "cover-gate", "vuln"} {
		if !strings.Contains(prereqs, gate) {
			t.Errorf("make ci must run the %q gate (prereqs: %q)", gate, prereqs)
		}
	}
	wf := readRepoFile(t, ".github/workflows/ci.yml")
	for _, tool := range []string{"golangci-lint", "govulncheck"} {
		if !strings.Contains(wf, tool) {
			t.Errorf("CI must install %q so the gate is enforced", tool)
		}
	}
}

// TestCIOSMatrix models REQ-CI-007: CI builds+tests both ship targets via an OS
// matrix; the macOS leg runs ci-darwin (no linux-only airgap gate).
func TestCIOSMatrix(t *testing.T) {
	wf := readRepoFile(t, ".github/workflows/ci.yml")
	for _, want := range []string{"matrix:", "ubuntu-latest", "macos-latest", "ci-darwin"} {
		if !strings.Contains(wf, want) {
			t.Errorf("CI workflow must define the OS matrix element %q", want)
		}
	}
	// ci-darwin must be the full pipeline minus the linux-only airgap gate.
	darwin := makeTargetPrereqs(t, readRepoFile(t, "Makefile"), "ci-darwin")
	if strings.Contains(darwin, "airgap") {
		t.Error("ci-darwin must NOT run the linux-only airgap gate")
	}
	for _, gate := range []string{"test", "race", "cover-gate"} {
		if !strings.Contains(darwin, gate) {
			t.Errorf("ci-darwin must still run %q", gate)
		}
	}
}

// TestCoverGateConfigured models REQ-CI-008: a coverage-regression gate exists
// with an explicit floor.
func TestCoverGateConfigured(t *testing.T) {
	mk := readRepoFile(t, "Makefile")
	if !strings.Contains(mk, "cover-gate:") {
		t.Error("Makefile must define a cover-gate target")
	}
	if !strings.Contains(mk, "COVER_MIN") {
		t.Error("cover-gate must be governed by an explicit COVER_MIN floor")
	}
}

// runAirgapRaw invokes the gate with no `--` separator and returns the exit code.
func runAirgapRaw(t *testing.T) int {
	t.Helper()
	cmd := exec.Command("bash", "scripts/verify-airgap.sh")
	cmd.Dir = repoRoot(t)
	if err := cmd.Run(); err != nil {
		if ee, ok := err.(*exec.ExitError); ok {
			return ee.ExitCode()
		}
		t.Fatalf("running airgap gate: %v", err)
	}
	return 0
}

// TestModelValidationRunbookPresent models REQ-DOCS-004: the real-model
// validation runbook exists and covers calibration, the preflight smoke, the
// digest gate, and golden-set ACR acceptance.
func TestModelValidationRunbookPresent(t *testing.T) {
	doc := readRepoFile(t, "docs/model-validation.md")
	for _, topic := range []string{"bench.sh", "smoke", "digest", "ACR", "golden"} {
		if !strings.Contains(doc, topic) {
			t.Errorf("model-validation runbook must cover %q", topic)
		}
	}
}

// TestDiscoverySkillPresent models REQ-FRAME-001: the discovery/framing skill
// exists with frontmatter and covers the double loop and its guardrails.
func TestDiscoverySkillPresent(t *testing.T) {
	s := readRepoFile(t, "skills/discovery/SKILL.md")
	if !strings.HasPrefix(s, "---") || !strings.Contains(s, "name: discovery") {
		t.Error("discovery skill must have valid frontmatter")
	}
	for _, topic := range []string{"define gate", "vertical slice", "propose", "parked", "human"} {
		if !strings.Contains(s, topic) {
			t.Errorf("discovery skill must cover %q", topic)
		}
	}
}
