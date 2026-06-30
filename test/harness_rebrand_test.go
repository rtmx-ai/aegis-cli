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
		"█████ █████ █████ ███ █████",              // OC-014: the aegis splash banner (logo.ts)
		"agentic coding in a closed enclave",       // the splash subtitle (home.tsx)
		"Built on OpenCode (MIT), llama.cpp (MIT)", // the splash package disclosures (home.tsx)
	} {
		if !strings.Contains(patch, want) {
			t.Errorf("rebrand patch must change branding to aegis: missing %q", want)
		}
	}
	// Gated: the built binary carries the aegis branding + splash (ASCII strings; bun escapes the
	// banner's block glyphs to █ so the wordmark is asserted via the patch above, not the binary).
	bin := filepath.Join(root, "deploy", "opencode", "bin", "opencode")
	if fi, err := os.Stat(bin); err == nil && fi.Mode().Perm()&0o111 != 0 {
		for _, sig := range []string{"aegis | ", "closed enclave", "Built on OpenCode"} {
			if grepBinaryCount(bin, sig) == 0 {
				t.Errorf("built opencode must carry the aegis branding/splash: missing %q", sig)
			}
		}
	}
	// Regression guard: useTheme() returns { theme } (an object); calling the result — theme() —
	// crashes the TUI home render with "is not a function". The splash must use the destructure +
	// property access (like logo.tsx), never a call. (Headless verify-env can't catch this.)
	// Scoped to the home.tsx hunk — footer.tsx legitimately uses its own theme() helper.
	homeHunk := patch
	if i := strings.Index(patch, "a/packages/tui/src/routes/home.tsx"); i >= 0 {
		homeHunk = patch[i:]
		if j := strings.Index(homeHunk, "\ndiff --git "); j >= 0 {
			homeHunk = homeHunk[:j]
		}
	}
	if strings.Contains(homeHunk, "theme().textMuted") {
		t.Error("home.tsx splash must use `const { theme } = useTheme()` + theme.textMuted, not theme() — TUI crash")
	}
}
