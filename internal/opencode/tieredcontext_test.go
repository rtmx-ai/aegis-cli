package opencode

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestLazyContextLoading → REQ-MEM-006: lean docs load into the base prompt while
// verbose docs are deferred to on-demand loading, keeping the base context small.
func TestLazyContextLoading(t *testing.T) {
	dir := t.TempDir()
	small := filepath.Join(dir, "small.md")
	big := filepath.Join(dir, "big.md")
	if err := os.WriteFile(small, []byte("small content"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(big, []byte("BIGCONTENT "+strings.Repeat("x", LazyThreshold+100)), 0o644); err != nil {
		t.Fatal(err)
	}
	sources := []MemorySource{{Name: "small", Path: small}, {Name: "big", Path: big}}

	base, onDemand := TierMemory(sources)
	if len(base) != 1 || base[0].Name != "small" {
		t.Errorf("small doc must be always-on base; got %+v", base)
	}
	if len(onDemand) != 1 || onDemand[0].Name != "big" {
		t.Errorf("large doc must be on-demand; got %+v", onDemand)
	}

	// The assembled base carries the lean doc, not the verbose one.
	assembled := AssembleMemory(base, 100000)
	if !strings.Contains(assembled, "small content") {
		t.Error("base must include the lean doc")
	}
	if strings.Contains(assembled, "BIGCONTENT") {
		t.Error("base must exclude the verbose doc (loaded on demand)")
	}
}
