// Package e2e is aegis's end-to-end security/sandbox gate harness: pure-Go command
// builders + output parsers + gate decisions for the offline stack (bubblewrap,
// gitleaks, govulncheck, syft, gosec). Each gate is unit-tested with fixtures so it
// runs without the external tools installed; live checks skip when a tool is absent.
package e2e

import "os/exec"

// SandboxOpts configures the bubblewrap sandbox for running agent-generated code
// (E2E-007): no network, a read-only system, and one writable workdir.
type SandboxOpts struct {
	Workdir   string // the only writable path (bound rw); "" = none
	NoNetwork bool   // --unshare-net (kernel netns: no interface -> egress impossible)
}

// SandboxCommand builds the bubblewrap argv that runs cmd in a locked sandbox:
// unshare pid/ipc/uts (+ net when NoNetwork), die-with-parent, a new session, a
// fresh proc/dev/tmpfs, a read-only system, and only Workdir writable (E2E-007). So
// agent-generated code from a golden-set run cannot escape, egress, or mutate the host.
func SandboxCommand(opts SandboxOpts, cmd ...string) []string {
	args := []string{
		"bwrap",
		"--unshare-pid", "--unshare-ipc", "--unshare-uts",
		"--die-with-parent", "--new-session",
		"--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp",
		"--ro-bind", "/usr", "/usr",
		"--ro-bind-try", "/bin", "/bin",
		"--ro-bind-try", "/lib", "/lib",
		"--ro-bind-try", "/lib64", "/lib64",
		"--ro-bind-try", "/etc/ssl", "/etc/ssl",
	}
	if opts.NoNetwork {
		args = append(args, "--unshare-net")
	}
	if opts.Workdir != "" {
		args = append(args, "--bind", opts.Workdir, opts.Workdir, "--chdir", opts.Workdir)
	}
	args = append(args, "--")
	return append(args, cmd...)
}

// SandboxAvailable reports whether bubblewrap is installed on PATH.
func SandboxAvailable() bool {
	_, err := exec.LookPath("bwrap")
	return err == nil
}
