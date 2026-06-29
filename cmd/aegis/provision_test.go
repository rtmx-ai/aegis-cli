package main

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestProvisionDownloadVerify → REQ-OC-024: downloadModel streams the catalog URL to dest, verifies
// the sha256, and refuses (deleting the partial) on mismatch — never serves unverified weights.
func TestProvisionDownloadVerify(t *testing.T) {
	payload := []byte("pretend gguf weights for the test")
	sum := sha256.Sum256(payload)
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		_, _ = w.Write(payload)
	}))
	defer srv.Close()

	dest := filepath.Join(t.TempDir(), "m.gguf")
	spec := provisionSpec{ID: "t", File: "m.gguf", URL: srv.URL, SHA256: hex.EncodeToString(sum[:]), Size: uint64(len(payload))}
	if err := downloadModel(spec, dest, io.Discard); err != nil {
		t.Fatalf("verified download must succeed: %v", err)
	}
	if b, _ := os.ReadFile(dest); string(b) != string(payload) {
		t.Error("downloaded content mismatch")
	}

	bad := filepath.Join(t.TempDir(), "bad.gguf")
	spec.SHA256 = "00deadbeef"
	if err := downloadModel(spec, bad, io.Discard); err == nil || !strings.Contains(err.Error(), "sha256") {
		t.Errorf("a sha256 mismatch must fail with a sha256 error, got %v", err)
	}
	if _, err := os.Stat(bad); err == nil {
		t.Error("a mismatched download must not be left on disk")
	}
}

// TestResolveProvisionSpec → REQ-OC-024: an explicit id resolves to its download spec; an empty id
// resolves the best-fitting US model; an unknown id errors.
func TestResolveProvisionSpec(t *testing.T) {
	root, _ := filepath.Abs("../..")
	t.Chdir(root)
	s, err := resolveProvisionSpec("gemma-4-26b-a4b")
	if err != nil {
		t.Fatalf("resolve gemma: %v", err)
	}
	if s.URL == "" || s.SHA256 == "" || s.File == "" {
		t.Errorf("spec must carry url/sha256/file, got %+v", s)
	}
	// empty id → the best-fitting US model, OR a clear "nothing fits" error on a constrained host
	// (the probe's measured bandwidth varies — both outcomes are correct, neither is a bug).
	best, err := resolveProvisionSpec("")
	if err != nil {
		if !strings.Contains(err.Error(), "fits") {
			t.Errorf("a no-fit result must explain nothing fits, got %v", err)
		}
	} else if best.ID == "" {
		t.Error("a best-fit success must return a model id")
	}
	if _, err := resolveProvisionSpec("nope-not-real"); err == nil {
		t.Error("unknown id must error")
	}
}

func TestResolveOrDownloadBrowse(t *testing.T) {
	tmp := t.TempDir()
	gguf := filepath.Join(tmp, "local.gguf")
	if err := os.WriteFile(gguf, []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	if got, ok := resolveOrDownload("", gguf, io.Discard, io.Discard); !ok || got != gguf {
		t.Errorf("browse should return the local path, got %q ok=%v", got, ok)
	}
	if _, ok := resolveOrDownload("", filepath.Join(tmp, "nope.gguf"), io.Discard, io.Discard); ok {
		t.Error("a missing browse path must fail")
	}
}

func TestModelDownloadDir(t *testing.T) {
	t.Setenv("MODEL_DOWNLOAD_DIR", "/tmp/x-models")
	if modelDownloadDir() != "/tmp/x-models" {
		t.Errorf("MODEL_DOWNLOAD_DIR override = %q", modelDownloadDir())
	}
}

// TestCmdProvisionMissingModel covers cmdProvision's resolve-failure path (no serve started).
func TestCmdProvisionMissingModel(t *testing.T) {
	var o, e bytes.Buffer
	if code := cmdProvision([]string{"--browse", "/no/such/model.gguf"}, &o, &e); code != 1 {
		t.Errorf("cmdProvision with a missing browse path must exit 1, got %d", code)
	}
}

// TestCmdProvisionUnknownId covers cmdProvision through the catalog resolve (the --id branch +
// resolveProvisionSpec lookup) to its error exit, without starting a server.
func TestCmdProvisionUnknownId(t *testing.T) {
	root, _ := filepath.Abs("../..")
	t.Chdir(root)
	var o, e bytes.Buffer
	if code := cmdProvision([]string{"--id", "no-such-model"}, &o, &e); code != 1 {
		t.Errorf("an unknown --id must exit 1, got %d", code)
	}
}
