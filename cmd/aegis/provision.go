package main

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/rtmx-ai/aegis-cli/internal/config"
)

// provisionSpec is the catalog facts needed to fetch + verify a model GGUF (REQ-OC-024).
type provisionSpec struct {
	ID     string `json:"id"`
	File   string `json:"file"`
	URL    string `json:"url"`
	SHA256 string `json:"sha256"`
	Size   uint64 `json:"size"`
}

// cmdProvision makes a model available end-to-end — the engine the in-TUI provisioning screen drives
// (the TUI spawns `aegis provision` and streams its progress). With no flags it provisions the
// best-fitting US model for this host; --id picks a catalog model; --browse sources a local GGUF
// without downloading. The download is the one place aegis egresses: operator-initiated, to a pinned
// catalog URL, sha256-verified, connected-host only. The serving runtime stays closed.
func cmdProvision(args []string, stdout, stderr io.Writer) int {
	fs := flag.NewFlagSet("provision", flag.ContinueOnError)
	fs.SetOutput(stderr)
	id := fs.String("id", "", "catalog model id to provision (default: the best-fitting US model)")
	browse := fs.String("browse", "", "source a local GGUF path instead of downloading")
	if err := fs.Parse(args); err != nil {
		return 2
	}
	cfg, err := config.Load("")
	if err != nil {
		cfg = config.Default()
	}

	gguf, ok := resolveOrDownload(*id, *browse, stdout, stderr)
	if !ok {
		return 1
	}

	seed, werr := writeSeedCalibration(gguf)
	if werr != nil {
		fmt.Fprintf(stderr, "aegis: provision: %v\n", werr)
		return 1
	}
	fmt.Fprintf(stdout, "aegis: provision: starting %s …\n", filepath.Base(gguf))
	cmd, berr := buildServeCommand(seed)
	if berr != nil {
		fmt.Fprintf(stderr, "aegis: provision: %v\n", berr)
		return 1
	}
	cmd.Stdout, cmd.Stderr = io.Discard, io.Discard
	if serr := cmd.Start(); serr != nil {
		fmt.Fprintf(stderr, "aegis: provision: launch model server: %v\n", serr)
		return 1
	}
	deadline := time.Now().Add(180 * time.Second)
	for time.Now().Before(deadline) {
		if endpointReady(cfg.Endpoint, 4*time.Second) {
			fmt.Fprintln(stdout, "aegis: provision: model ready")
			return 0
		}
		time.Sleep(2 * time.Second)
	}
	fmt.Fprintln(stderr, "aegis: provision: model did not become ready within 180s")
	return 1
}

// resolveOrDownload returns a ready local GGUF path: a browsed path as-is, an already-present catalog
// file (size-matched), or a freshly downloaded + sha256-verified one. ok=false on failure.
func resolveOrDownload(id, browse string, stdout, stderr io.Writer) (string, bool) {
	if browse != "" {
		if _, err := os.Stat(browse); err != nil {
			fmt.Fprintf(stderr, "aegis: provision: %v\n", err)
			return "", false
		}
		fmt.Fprintf(stdout, "aegis: provision: sourcing local model %s\n", filepath.Base(browse))
		return browse, true
	}
	spec, err := resolveProvisionSpec(id)
	if err != nil {
		fmt.Fprintf(stderr, "aegis: provision: %v\n", err)
		return "", false
	}
	dest := filepath.Join(modelDownloadDir(), spec.File)
	if fi, serr := os.Stat(dest); serr == nil && uint64(fi.Size()) == spec.Size {
		fmt.Fprintf(stdout, "aegis: provision: %s already present\n", spec.File)
		return dest, true
	}
	fmt.Fprintf(stdout, "aegis: provision: downloading %s (%.1f GB) from the catalog…\n", spec.ID, float64(spec.Size)/1e9)
	if derr := downloadModel(spec, dest, stdout); derr != nil {
		fmt.Fprintf(stderr, "aegis: provision: %v\n", derr)
		return "", false
	}
	return dest, true
}

// resolveProvisionSpec returns the download spec for id, or — when id is empty — the best-fitting US
// model for this host (the profiler's recommendation). Unknown ids error.
func resolveProvisionSpec(id string) (provisionSpec, error) {
	if id == "" {
		rec, err := computeRecommendation(16384)
		if err != nil {
			return provisionSpec{}, err
		}
		id = rec.Interactive
		if id == "" {
			id = rec.Unattended
		}
		if id == "" {
			return provisionSpec{}, fmt.Errorf("no US-origin model fits this host — pass --browse <path.gguf> or --id <catalog-id>")
		}
	}
	b, err := catalogBytes()
	if err != nil {
		return provisionSpec{}, fmt.Errorf("model catalog not found: %w", err)
	}
	var cat struct {
		Models []provisionSpec `json:"models"`
	}
	if err := json.Unmarshal(b, &cat); err != nil {
		return provisionSpec{}, fmt.Errorf("parse catalog: %w", err)
	}
	for _, m := range cat.Models {
		if m.ID == id {
			if m.URL == "" || m.SHA256 == "" {
				return provisionSpec{}, fmt.Errorf("catalog model %q has no download URL/sha256", id)
			}
			m.URL = modelGardenURL(m.File, m.URL) // OC-040: enterprise mirror override
			return m, nil
		}
	}
	return provisionSpec{}, fmt.Errorf("unknown catalog model %q", id)
}

// catalogBytes returns the model catalog — an operator-supplied AEGIS_CATALOG if set (an enterprise
// model garden), else the built-in catalog (OC-040).
func catalogBytes() ([]byte, error) {
	if p := os.Getenv("AEGIS_CATALOG"); p != "" {
		return os.ReadFile(p)
	}
	return deployFileBytes("deploy/models/catalog.json")
}

// modelGardenURL rewrites a catalog download URL to the operator's mirror (AEGIS_MODEL_GARDEN base +
// the pinned filename) when set, keeping the catalog's sha256 so integrity verification is unchanged
// (OC-040).
func modelGardenURL(file, catalogURL string) string {
	if g := os.Getenv("AEGIS_MODEL_GARDEN"); g != "" {
		return strings.TrimRight(g, "/") + "/" + file
	}
	return catalogURL
}

// downloadModel streams spec.URL to dest with progress, verifying the sha256 before committing; a
// mismatch removes the partial file and fails — never serve unverified weights.
func downloadModel(spec provisionSpec, dest string, progress io.Writer) error {
	if err := os.MkdirAll(filepath.Dir(dest), 0o755); err != nil {
		return err
	}
	resp, err := http.Get(spec.URL) //nolint:gosec // pinned catalog URL, sha256-verified below
	if err != nil {
		return fmt.Errorf("download %s: %w", spec.URL, err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("download %s: HTTP %d", spec.URL, resp.StatusCode)
	}
	tmp := dest + ".part"
	f, err := os.Create(tmp)
	if err != nil {
		return err
	}
	h := sha256.New()
	pt := &progressTracker{total: spec.Size, w: progress}
	if _, err := io.Copy(io.MultiWriter(f, h, pt), resp.Body); err != nil {
		f.Close()
		os.Remove(tmp)
		return err
	}
	f.Close()
	if got := hex.EncodeToString(h.Sum(nil)); got != spec.SHA256 {
		os.Remove(tmp)
		return fmt.Errorf("sha256 mismatch (want %s, got %s) — refusing to serve unverified weights", spec.SHA256, got)
	}
	return os.Rename(tmp, dest)
}

// progressTracker emits a download-progress line at most once a second (parseable by the TUI screen).
type progressTracker struct {
	total, done uint64
	start, last time.Time
	w           io.Writer
}

func (p *progressTracker) Write(b []byte) (int, error) {
	p.done += uint64(len(b))
	now := time.Now()
	if p.start.IsZero() {
		p.start = now
	}
	if now.Sub(p.last) > time.Second || (p.total > 0 && p.done >= p.total) {
		p.last = now
		pct := 0.0
		if p.total > 0 {
			pct = 100 * float64(p.done) / float64(p.total)
		}
		mbps, eta := 0.0, ""
		if el := now.Sub(p.start).Seconds(); el > 0 {
			mbps = float64(p.done) / 1e6 / el
			if mbps > 0 && p.total > p.done {
				secs := float64(p.total-p.done) / 1e6 / mbps
				eta = " ETA " + (time.Duration(secs) * time.Second).Round(time.Second).String()
			}
		}
		// The TUI parses the "downloaded X/Y GB (Z%)" prefix; the bar + rate follow for CLI-direct use.
		fmt.Fprintf(p.w, "aegis: provision: downloaded %.1f/%.1f GB (%.0f%%) %s %.0f MB/s%s\n",
			float64(p.done)/1e9, float64(p.total)/1e9, pct, progressBar(pct), mbps, eta)
	}
	return len(b), nil
}

// progressBar renders a fixed-width unicode bar for a 0-100 percentage.
func progressBar(pct float64) string {
	const width = 20
	filled := int(pct / 100 * float64(width))
	if filled < 0 {
		filled = 0
	}
	if filled > width {
		filled = width
	}
	return "[" + strings.Repeat("█", filled) + strings.Repeat("░", width-filled) + "]"
}

func modelDownloadDir() string {
	if d := os.Getenv("MODEL_DOWNLOAD_DIR"); d != "" {
		return d
	}
	if home, err := os.UserHomeDir(); err == nil {
		return filepath.Join(home, "models")
	}
	return "models"
}

// bestFitCard renders the best-fitting US model as a one-line card for the in-TUI provisioning screen
// (AEGIS_BEST_FIT, OC-022), or "" when nothing fits this host.
func bestFitCard() string {
	spec, err := resolveProvisionSpec("")
	if err != nil {
		return ""
	}
	return fmt.Sprintf("%s · US-origin · ~%.0f GB", spec.ID, float64(spec.Size)/1e9)
}
