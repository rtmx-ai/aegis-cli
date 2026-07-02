package offline

import (
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// TestSiteDocsIA → REQ-SITE-005: the doc-suite manifest wires every rtmx.ai aegis page
// to canonical aegis-cli docs (reference, don't duplicate) with no dangling references.
// The live sidebar check (AEGIS_LIVE_SITE=1) confirms the section is actually deployed;
// it is off by default so the offline suite + egress gate never make a network call.
func TestSiteDocsIA(t *testing.T) {
	var m struct {
		Pages []struct {
			Slug        string   `json:"slug"`
			Title       string   `json:"title"`
			Requirement string   `json:"requirement"`
			Canonical   []string `json:"canonical"`
		} `json:"pages"`
	}
	if err := json.Unmarshal([]byte(readRepoFile(t, "docs/site-manifest.json")), &m); err != nil {
		t.Fatalf("site-manifest.json must parse: %v", err)
	}
	if len(m.Pages) < 7 {
		t.Errorf("manifest must cover the doc-suite sections (>=7 pages), got %d", len(m.Pages))
	}

	root := repoRoot(t)
	seen := map[string]bool{}
	for _, p := range m.Pages {
		if p.Slug == "" || p.Title == "" || p.Requirement == "" {
			t.Errorf("page has empty slug/title/requirement: %+v", p)
		}
		seen[p.Slug] = true
		if len(p.Canonical) == 0 {
			t.Errorf("page %q references no canonical doc (reference, don't duplicate)", p.Slug)
		}
		for _, c := range p.Canonical {
			if _, err := os.Stat(filepath.Join(root, c)); err != nil {
				t.Errorf("page %q references missing canonical doc %q", p.Slug, c)
			}
		}
	}
	if !seen["aegis"] {
		t.Error("manifest must include the aegis Overview page (slug 'aegis')")
	}

	// Live: the section pages are deployed on rtmx.ai (env-gated; no offline egress).
	t.Run("live_sidebar", func(t *testing.T) {
		if os.Getenv("AEGIS_LIVE_SITE") != "1" {
			t.Skip("live sidebar check disabled; set AEGIS_LIVE_SITE=1")
		}
		if _, err := exec.LookPath("gh"); err != nil {
			t.Skip("gh not available")
		}
		for _, slug := range []string{"aegis/using", "aegis/operator", "aegis/security"} {
			path := "src/content/docs/" + slug + ".md"
			out, err := exec.Command("gh", "api", "repos/rtmx-ai/rtmx.ai/contents/"+path, "--jq", ".name").CombinedOutput()
			if err != nil || !strings.Contains(string(out), filepath.Base(slug)) {
				t.Errorf("doc-suite page %q not deployed on rtmx.ai: %v %s", path, err, out)
			}
		}
	})
}
