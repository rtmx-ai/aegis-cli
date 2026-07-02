package offline

import (
	"encoding/base64"
	"os"
	"os/exec"
	"strings"
	"testing"
)

// assertSitePageLive verifies a deployed rtmx.ai aegis doc page is filled (not a stub)
// and contains the given content markers. Off by default (AEGIS_LIVE_SITE=1) so the
// offline suite + the egress gate never make a network call.
func assertSitePageLive(t *testing.T, slug string, markers ...string) {
	t.Helper()
	if os.Getenv("AEGIS_LIVE_SITE") != "1" {
		t.Skip("live site check disabled; set AEGIS_LIVE_SITE=1")
	}
	if _, err := exec.LookPath("gh"); err != nil {
		t.Skip("gh not available")
	}
	out, err := exec.Command("gh", "api", "repos/rtmx-ai/rtmx.ai/contents/src/content/docs/"+slug+".md", "--jq", ".content").CombinedOutput()
	if err != nil {
		t.Fatalf("fetch %s: %v", slug, err)
	}
	dec, _ := base64.StdEncoding.DecodeString(strings.ReplaceAll(strings.TrimSpace(string(out)), "\n", ""))
	body := string(dec)
	for _, m := range markers {
		if !strings.Contains(body, m) {
			t.Errorf("page %s not filled — missing expected content %q", slug, m)
		}
	}
}

// TestSiteGettingStarted → REQ-SITE-006
func TestSiteGettingStarted(t *testing.T) {
	assertSitePageLive(t, "aegis/getting-started", "brew install", "verify-env", "OpenCode TUI")
}

// TestSiteUsingAegis → REQ-SITE-007
func TestSiteUsingAegis(t *testing.T) {
	assertSitePageLive(t, "aegis/using", "rtmx intent loop", "aegis propose", "repo map")
}

// TestSiteOperatorDocs → REQ-SITE-008
func TestSiteOperatorDocs(t *testing.T) {
	assertSitePageLive(t, "aegis/operator", "calibration.json", "circuit breaker", "verify-env")
}

// TestSiteSecurityDocs → REQ-SITE-009
func TestSiteSecurityDocs(t *testing.T) {
	assertSitePageLive(t, "aegis/security", "EGRESS", "minisign", "bubblewrap")
}

// TestSiteReferenceDocs → REQ-SITE-010
func TestSiteReferenceDocs(t *testing.T) {
	assertSitePageLive(t, "aegis/reference", "ACR", "control loop", "bake-off")
}

// TestSiteEvaluateDocs → REQ-SITE-011
func TestSiteEvaluateDocs(t *testing.T) {
	assertSitePageLive(t, "aegis/evaluate", "air-gapped", "OpenCode", "roadmap")
}
