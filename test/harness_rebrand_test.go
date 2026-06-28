package offline

import (
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
)

func grepBinaryCount(bin, sig string) int {
	out, _ := exec.Command("bash", "-c", "grep -c -a -- "+strconv.Quote(sig)+" "+strconv.Quote(bin)+" || true").Output()
	n, _ := strconv.Atoi(strings.TrimSpace(string(out)))
	return n
}

// TestHarnessRebranded → REQ-OC-014: the app is rebranded "aegis" via an OC-017 patch over the
// pinned source — terminal title, CLI scriptName, the wordmark, and the HTTP user-agent.
func TestHarnessRebranded(t *testing.T) {
	root := repoRoot(t)
	patch := readRepoFile(t, "deploy/opencode/patches/20-rebrand-aegis.patch")
	for _, want := range []string{
		`setTerminalTitle("aegis")`, // terminal title
		`.scriptName("aegis")`,      // CLI name
		"user-agent=aegis/",         // HTTP user-agent
	} {
		if !strings.Contains(patch, want) {
			t.Errorf("rebrand patch must change branding to aegis: missing %q", want)
		}
	}
	// Gated: the built binary carries the aegis branding, not the OpenCode wordmark.
	bin := filepath.Join(root, "deploy", "opencode", "bin", "opencode")
	if fi, err := os.Stat(bin); err == nil && fi.Mode().Perm()&0o111 != 0 {
		if grepBinaryCount(bin, "aegis | ") == 0 {
			t.Error("built opencode must carry the aegis terminal title")
		}
		if grepBinaryCount(bin, "█▀▀█ █▀▀█ █▀▀█ █▀▀▄") > 0 {
			t.Error("built opencode still carries the OPENCODE wordmark — rebrand not applied")
		}
	}
}
