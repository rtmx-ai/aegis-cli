package offline

import (
	"encoding/json"
	"strings"
	"testing"
)

// TestDevstralCandidate → REQ-MODEL-009: Devstral Small (Mistral AI, France; Apache-2.0) is a sha-pinned
// catalog candidate — the non-PRC agentic coder for the bake-off — with FR explicitly opted into the
// origin policy, and the OpenCode whitelist entry present with tool-calling enabled and reasoning OFF.
// Also guards the gemma reasoning revert (reasoning-on amplified the content->reasoning empty-output
// failure the research documented).
func TestDevstralCandidate(t *testing.T) {
	// --- catalog entry (both the repo source and the embedded copy stay in sync via OC-033) ---
	for _, path := range []string{"deploy/models/catalog.json", "cmd/aegis/deploydata/catalog.json"} {
		var cat struct {
			Models []struct {
				ID     string `json:"id"`
				File   string `json:"file"`
				URL    string `json:"url"`
				SHA256 string `json:"sha256"`
				Size   int64  `json:"size"`
				Origin string `json:"origin"`
			} `json:"models"`
		}
		if err := json.Unmarshal([]byte(readRepoFile(t, path)), &cat); err != nil {
			t.Fatalf("%s: %v", path, err)
		}
		var d *struct {
			ID, File, URL, SHA256, Origin string
			Size                          int64
		}
		for _, m := range cat.Models {
			if m.ID == "devstral-small-2507" {
				d = &struct {
					ID, File, URL, SHA256, Origin string
					Size                          int64
				}{m.ID, m.File, m.URL, m.SHA256, m.Origin, m.Size}
			}
		}
		if d == nil {
			t.Fatalf("%s: devstral-small-2507 not in catalog", path)
		}
		if d.Origin != "FR" {
			t.Errorf("%s: Devstral origin=%q, want FR (Mistral/France)", path, d.Origin)
		}
		if len(d.SHA256) != 64 {
			t.Errorf("%s: Devstral must be sha256-pinned; got %q", path, d.SHA256)
		}
		if d.Size <= 0 || !strings.Contains(d.URL, "IQ4_XS") || !strings.Contains(d.File, "IQ4_XS") {
			t.Errorf("%s: Devstral must pin the IQ4_XS build with a real size; got file=%q size=%d", path, d.File, d.Size)
		}
	}

	// --- origin policy: FR explicitly allowed (a deliberate, auditable non-US opt-in) ---
	for _, path := range []string{"deploy/models/origin-policy.json", "cmd/aegis/deploydata/origin-policy.json"} {
		var pol struct {
			Default   string            `json:"default"`
			Countries map[string]string `json:"countries"`
		}
		if err := json.Unmarshal([]byte(readRepoFile(t, path)), &pol); err != nil {
			t.Fatalf("%s: %v", path, err)
		}
		if pol.Default != "deny" {
			t.Errorf("%s: policy must stay default-deny; got %q", path, pol.Default)
		}
		if pol.Countries["FR"] != "allow" {
			t.Errorf("%s: FR must be explicitly allowed for Devstral; got %q", path, pol.Countries["FR"])
		}
		if pol.Countries["CN"] == "allow" {
			t.Errorf("%s: CN must NOT be allowed (non-PRC posture)", path)
		}
	}

	// --- OpenCode whitelist: Devstral present, tool-calling on; gemma reasoning reverted to false ---
	var wl struct {
		Aegis struct {
			Models map[string]struct {
				ToolCall  bool `json:"tool_call"`
				Reasoning bool `json:"reasoning"`
			} `json:"models"`
		} `json:"aegis"`
	}
	if err := json.Unmarshal([]byte(readRepoFile(t, "deploy/opencode/models-whitelist.json")), &wl); err != nil {
		t.Fatalf("whitelist: %v", err)
	}
	dev, ok := wl.Aegis.Models["devstral-small-2507"]
	if !ok {
		t.Fatal("whitelist missing devstral-small-2507")
	}
	if !dev.ToolCall {
		t.Error("Devstral must have tool_call=true (agentic coder)")
	}
	if dev.Reasoning {
		t.Error("Devstral is not a reasoning model; reasoning must be false")
	}
	if g, ok := wl.Aegis.Models["gemma-4-26b-a4b"]; !ok || g.Reasoning {
		t.Error("gemma reasoning must be reverted to false (reasoning-on amplified the empty-output failure)")
	}
}
