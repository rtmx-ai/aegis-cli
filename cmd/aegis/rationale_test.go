package main

import (
	"strings"
	"testing"
)

// TestRecommendationRationale → REQ-OC-042: the provisioning screen explains WHY the recommended model
// is recommended — capability, host-fit, and US origin — so the operator understands the larger pick.
func TestRecommendationRationale(t *testing.T) {
	got := rationaleFor("gemma-4-26b-a4b", "interactive")
	for _, want := range []string{"gemma-4-26b-a4b", "US-origin", "Recommended", "interactive", "fits this host"} {
		if !strings.Contains(got, want) {
			t.Errorf("rationale missing %q: %s", want, got)
		}
	}
	if rationaleFor("", "interactive") != "" {
		t.Error("no recommended model → empty rationale")
	}
}
