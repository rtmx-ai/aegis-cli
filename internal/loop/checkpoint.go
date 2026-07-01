package loop

import (
	"os/exec"
	"strings"
)

// Checkpoint snapshots the workspace into a SHADOW git repo (a separate git-dir),
// so a bad mid-task edit can be rolled back without touching the project's own git
// history (LONGRUN-007; opencode's /undo shadow-git pattern). Returns the snapshot
// commit SHA. Idempotent: re-inits the shadow repo if needed.
func Checkpoint(workspace, gitDir string) (string, error) {
	if err := gitShadow(workspace, gitDir, "init", "-q").Run(); err != nil {
		return "", err
	}
	if err := gitShadow(workspace, gitDir, "add", "-A").Run(); err != nil {
		return "", err
	}
	if err := gitShadow(workspace, gitDir, "commit", "-q", "--allow-empty", "-m", "aegis checkpoint").Run(); err != nil {
		return "", err
	}
	out, err := gitShadow(workspace, gitDir, "rev-parse", "HEAD").Output()
	return strings.TrimSpace(string(out)), err
}

// Rollback restores the workspace to a prior checkpoint SHA — tracked files reset,
// files created since the checkpoint removed — recovering from a bad edit without
// losing the run (LONGRUN-007).
func Rollback(workspace, gitDir, sha string) error {
	if err := gitShadow(workspace, gitDir, "reset", "-q", "--hard", sha).Run(); err != nil {
		return err
	}
	return gitShadow(workspace, gitDir, "clean", "-fdq").Run()
}

// gitShadow builds a git command against a shadow git-dir over the workspace tree,
// with a fixed identity so it never depends on the host's git config.
func gitShadow(workspace, gitDir string, args ...string) *exec.Cmd {
	full := append([]string{
		"--git-dir=" + gitDir, "--work-tree=" + workspace,
		"-c", "user.email=aegis@local", "-c", "user.name=aegis", "-c", "commit.gpgsign=false",
	}, args...)
	cmd := exec.Command("git", full...)
	cmd.Dir = workspace
	return cmd
}
