//go:build !windows

package main

import (
	"os/exec"
	"syscall"
)

// setServeProcAttr puts a spawned server in its OWN process group, so killServe can tear down the whole
// tree (a `nice` → `llama-server` chain, and any llama-server children) — not just the parent. On macOS
// `nice` does not execve-replace itself, so killing the parent pid orphans llama-server, which then holds
// the port and makes the next bake-off candidate measure the lingering model (BENCH-012).
func setServeProcAttr(cmd *exec.Cmd) {
	cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}
}

// killServe kills the server's whole process group, then reaps it.
func killServe(cmd *exec.Cmd) {
	if cmd == nil || cmd.Process == nil {
		return
	}
	if pgid, err := syscall.Getpgid(cmd.Process.Pid); err == nil {
		_ = syscall.Kill(-pgid, syscall.SIGKILL)
	} else {
		_ = cmd.Process.Kill()
	}
	_ = cmd.Wait()
}
