package offline

import (
	"bytes"
	"os"
	"path/filepath"
	"testing"
)

// TestEmbeddedDeployInSync → REQ-OC-033: the embedded copies (cmd/aegis/deploydata) must stay
// byte-identical to deploy/models so an installed binary serves the same data as the repo.
func TestEmbeddedDeployInSync(t *testing.T) {
	root := repoRoot(t)
	for _, f := range []string{"catalog.json", "origin-policy.json", "MODEL_REF"} {
		src, err := os.ReadFile(filepath.Join(root, "deploy", "models", f))
		if err != nil {
			t.Fatalf("read deploy/models/%s: %v", f, err)
		}
		emb, err := os.ReadFile(filepath.Join(root, "cmd", "aegis", "deploydata", f))
		if err != nil {
			t.Fatalf("read cmd/aegis/deploydata/%s: %v", f, err)
		}
		if !bytes.Equal(src, emb) {
			t.Errorf("cmd/aegis/deploydata/%s drifted from deploy/models/%s — re-copy it", f, f)
		}
	}
}
