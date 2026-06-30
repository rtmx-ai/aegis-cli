package opencode

import (
	"testing"
	"time"
)

// TestServeReadyTimeoutGenerous → REQ-OC-037: the opencode-serve readiness bound must stay generous
// (>= 60s). A 30s bound flaked the ITAR egress gate when the first bootstrap (plugin install) ran
// under test-suite CPU contention.
func TestServeReadyTimeoutGenerous(t *testing.T) {
	if ServeReadyTimeout < 60*time.Second {
		t.Errorf("ServeReadyTimeout = %s; must be >= 60s to avoid flaking the ITAR gate under load", ServeReadyTimeout)
	}
}
