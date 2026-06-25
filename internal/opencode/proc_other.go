//go:build !unix

package opencode

import "os/exec"

// setProcGroup is a no-op on platforms without POSIX process groups; the default
// context cancellation (kill the process) applies.
func setProcGroup(cmd *exec.Cmd) {}
