package main

import (
	"bytes"
	"path/filepath"
	"strings"
	"testing"
)

// TestCmdProfile → PROFILE-001: `aegis profile` probes the host + emits a recommendation, read-only.
func TestCmdProfile(t *testing.T) {
	root, err := filepath.Abs("../..")
	if err != nil {
		t.Fatal(err)
	}
	t.Chdir(root)                 // catalog + policy resolve cwd-relative from the repo
	t.Setenv("HOME", t.TempDir()) // don't clobber the real ~/.config/aegis/profile.json
	var out, errb bytes.Buffer
	if code := cmdProfile(nil, &out, &errb); code != 0 {
		t.Fatalf("cmdProfile exit %d, stderr: %s", code, errb.String())
	}
	s := out.String()
	if !strings.Contains(s, "bandwidth") || !strings.Contains(s, "recommendation:") {
		t.Errorf("profile output must report bandwidth + a recommendation:\n%s", s)
	}
}
