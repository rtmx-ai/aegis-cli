//go:build unix

package opencode

import (
	"os/exec"
	"syscall"
)

// setProcGroup puts the child in its own process group and, on context cancel,
// kills the whole group — so `opencode run` and any children (model connections,
// shells) are torn down promptly when a run hits its budget (RUNQ-001).
func setProcGroup(cmd *exec.Cmd) {
	cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}
	cmd.Cancel = func() error {
		if cmd.Process != nil {
			_ = syscall.Kill(-cmd.Process.Pid, syscall.SIGKILL)
		}
		return nil
	}
}
