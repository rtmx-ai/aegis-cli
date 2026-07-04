//go:build windows

package main

import "os/exec"

// setServeProcAttr is a no-op on Windows (process groups differ; the bake-off serve path is not a
// Windows target). Kept so cmd/aegis cross-compiles for windows (WIN-001).
func setServeProcAttr(cmd *exec.Cmd) {}

// killServe kills the server process on Windows.
func killServe(cmd *exec.Cmd) {
	if cmd == nil || cmd.Process == nil {
		return
	}
	_ = cmd.Process.Kill()
	_ = cmd.Wait()
}
