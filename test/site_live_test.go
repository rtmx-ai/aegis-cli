package offline

import (
	"encoding/base64"
	"os"
	"os/exec"
	"strings"
	"testing"
)

// TestSitePRMerged → REQ-SITE-002: the rtmx.ai integration is live on main — aegis-cli
// is a submodule and the aegis docs page exists. LIVE external check, OFF by default:
// skipped unless AEGIS_LIVE_SITE=1, so the offline suite (and the egress gate) never make
// a network call. Verify SITE-002 with:
//
//	AEGIS_LIVE_SITE=1 go test ./test/ -run TestSitePRMerged
func TestSitePRMerged(t *testing.T) {
	if os.Getenv("AEGIS_LIVE_SITE") != "1" {
		t.Skip("live site check disabled; set AEGIS_LIVE_SITE=1 (kept off so the offline suite never egresses)")
	}
	if _, err := exec.LookPath("gh"); err != nil {
		t.Skip("gh not available")
	}

	// The aegis docs page is on rtmx.ai main.
	if out, err := exec.Command("gh", "api", "repos/rtmx-ai/rtmx.ai/contents/src/content/docs/aegis.md", "--jq", ".name").CombinedOutput(); err != nil || !strings.Contains(string(out), "aegis.md") {
		t.Errorf("aegis docs page not on rtmx.ai main: %v %s", err, out)
	}

	// aegis-cli is registered as a submodule.
	gm, err := exec.Command("gh", "api", "repos/rtmx-ai/rtmx.ai/contents/.gitmodules", "--jq", ".content").CombinedOutput()
	if err != nil {
		t.Fatalf("read .gitmodules: %v", err)
	}
	dec, _ := base64.StdEncoding.DecodeString(strings.ReplaceAll(strings.TrimSpace(string(gm)), "\n", ""))
	if !strings.Contains(string(dec), "aegis-cli") {
		t.Error("rtmx.ai .gitmodules must include the aegis-cli submodule")
	}
}
