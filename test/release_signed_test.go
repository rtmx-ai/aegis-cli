package offline

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// TestReleaseSigned → REQ-REL-001: a release signing key is provisioned — its PUBLIC key is
// committed to deploy/release/, the release signs SHA256SUMS, and `make verify-release`
// verifies against it. The committed pubkey's secret half is held off-repo (a CI/host
// secret), so this asserts the public key + the sign/verify wiring, and proves the minisign
// round-trip works (with a throwaway key) so the release-time signature will verify.
func TestReleaseSigned(t *testing.T) {
	root := repoRoot(t)
	pub := filepath.Join(root, "deploy", "release", "aegis-minisign.pub")
	b, err := os.ReadFile(pub)
	if err != nil {
		t.Fatalf("release signing public key not committed at %s: %v", pub, err)
	}
	lines := strings.Split(strings.TrimSpace(string(b)), "\n")
	if len(lines) < 2 || !strings.HasPrefix(lines[0], "untrusted comment:") || !strings.HasPrefix(lines[1], "RW") {
		t.Errorf("deploy/release/aegis-minisign.pub is not a minisign public key:\n%s", b)
	}
	if rel := readRepoFile(t, "scripts/release.sh"); !strings.Contains(rel, "minisign -S") {
		t.Error("release.sh must sign SHA256SUMS (minisign -S)")
	}
	mk := readRepoFile(t, "Makefile")
	if !strings.Contains(mk, "aegis-minisign.pub") || !strings.Contains(mk, "minisign -Vm") {
		t.Error("make verify-release must verify SHA256SUMS against deploy/release/aegis-minisign.pub")
	}

	// Prove the sign->verify mechanism end-to-end (throwaway key), so a release signed with
	// the committed key's secret half will verify the same way.
	if _, err := exec.LookPath("minisign"); err != nil {
		t.Skip("minisign not installed; static checks done")
	}
	dir := t.TempDir()
	sk, pk := filepath.Join(dir, "k.key"), filepath.Join(dir, "k.pub")
	if out, err := exec.Command("minisign", "-G", "-W", "-s", sk, "-p", pk).CombinedOutput(); err != nil {
		t.Skipf("minisign keygen unavailable here: %v\n%s", err, out)
	}
	msg := filepath.Join(dir, "SHA256SUMS")
	if err := os.WriteFile(msg, []byte("deadbeef  aegis\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if out, err := exec.Command("minisign", "-S", "-s", sk, "-m", msg).CombinedOutput(); err != nil {
		t.Fatalf("minisign sign: %v\n%s", err, out)
	}
	if out, err := exec.Command("minisign", "-Vm", msg, "-p", pk).CombinedOutput(); err != nil {
		t.Fatalf("minisign sign->verify round-trip failed: %v\n%s", err, out)
	}
}
