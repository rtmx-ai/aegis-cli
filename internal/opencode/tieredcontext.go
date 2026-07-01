package opencode

import "os"

// LazyThreshold is the per-source byte cap: a memory source larger than this is
// loaded lazily (on demand, e.g. via a slash command) rather than injected into
// the base prompt (MEM-006) — keeping the small model's base context lean, with
// verbose tool/procedure docs pulled only when invoked.
const LazyThreshold = 2000

// TierMemory splits sources into always-on (small, injected into the base prompt)
// and on-demand (large, loaded only when invoked) — the lazy/tiered loading of
// MEM-006. Precedence order is preserved within each tier.
func TierMemory(sources []MemorySource) (base, onDemand []MemorySource) {
	for _, s := range sources {
		if fileSize(s.Path) > LazyThreshold {
			onDemand = append(onDemand, s)
		} else {
			base = append(base, s)
		}
	}
	return base, onDemand
}

func fileSize(path string) int64 {
	fi, err := os.Stat(path)
	if err != nil {
		return 0
	}
	return fi.Size()
}
