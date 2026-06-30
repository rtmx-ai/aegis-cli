package main

import (
	"path/filepath"
	"strings"
	"testing"
)

// TestDeployFileBytesEmbedded → REQ-OC-033: with no deploy/ on disk (an installed binary run from any
// directory), the catalog still resolves from the embedded copy.
func TestDeployFileBytesEmbedded(t *testing.T) {
	t.Chdir(t.TempDir())
	b, err := deployFileBytes(filepath.Join("deploy", "models", "catalog.json"))
	if err != nil || len(b) == 0 {
		t.Fatalf("embedded catalog must resolve with no file present: err=%v len=%d", err, len(b))
	}
	if !strings.Contains(string(b), "gemma-4-26b-a4b") {
		t.Errorf("embedded catalog looks wrong (missing the recommended model)")
	}
	if _, perr := aegisOriginPolicy(); perr != nil {
		t.Errorf("origin policy must resolve from the embedded default: %v", perr)
	}
}
