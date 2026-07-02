package offline

import (
	"encoding/base64"
	"os"
	"os/exec"
	"strings"
	"testing"
)

// TestSiteDownloadsLive → REQ-SITE-003: rtmx.ai surfaces aegis download references
// pointing to signed releases (Homebrew + .deb + minisign verify), and a signed
// release is actually live with those artifacts. Off by default (AEGIS_LIVE_SITE)
// so the offline suite + egress gate never make a network call.
func TestSiteDownloadsLive(t *testing.T) {
	if os.Getenv("AEGIS_LIVE_SITE") != "1" {
		t.Skip("live site check disabled; set AEGIS_LIVE_SITE=1")
	}
	if _, err := exec.LookPath("gh"); err != nil {
		t.Skip("gh not available")
	}

	// The aegis page references downloads + signed-release verification.
	out, err := exec.Command("gh", "api", "repos/rtmx-ai/rtmx.ai/contents/src/content/docs/aegis.md", "--jq", ".content").CombinedOutput()
	if err != nil {
		t.Fatalf("fetch aegis page: %v", err)
	}
	dec, _ := base64.StdEncoding.DecodeString(strings.ReplaceAll(strings.TrimSpace(string(out)), "\n", ""))
	page := string(dec)
	for _, m := range []string{"brew install", ".deb", "minisign"} {
		if !strings.Contains(page, m) {
			t.Errorf("aegis page missing download reference %q", m)
		}
	}

	// A signed release is live with a .deb + a minisign signature.
	rel, err := exec.Command("gh", "release", "view", "--repo", "rtmx-ai/aegis-cli", "--json", "assets", "--jq", ".assets[].name").CombinedOutput()
	if err != nil {
		t.Fatalf("release view: %v", err)
	}
	names := string(rel)
	if !strings.Contains(names, ".deb") || !strings.Contains(names, ".minisig") {
		t.Errorf("latest signed release must include a .deb and a minisig; got:\n%s", names)
	}
}
