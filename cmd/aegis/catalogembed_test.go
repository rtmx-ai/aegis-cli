package main

import "testing"

// TestCatalogIDResolvesFromEmbedded → PERF-007: an installed binary has no catalog FILE — only the
// embedded one — so catalogIDForGGUF must resolve via the embedded catalog. Otherwise the model label
// silently falls back to the default (the M5 symptom: aegis status shows the GGUF, the TUI shows the
// default "gemma4-qat:32k").
func TestCatalogIDResolvesFromEmbedded(t *testing.T) {
	t.Chdir(t.TempDir()) // no catalog file in cwd — simulate an installed binary
	t.Setenv("AEGIS_CATALOG", "")
	if got := catalogIDForGGUF("gemma-4-26B-A4B-it-qat-UD-Q4_K_XL.gguf"); got != "gemma-4-26b-a4b" {
		t.Errorf("catalogIDForGGUF via embedded catalog = %q; want gemma-4-26b-a4b", got)
	}
}
