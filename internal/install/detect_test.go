package install

import (
	"fmt"
	"testing"
)

// stubSources is an injectable Sources backed by in-memory fixtures so tests
// never read the real host.
type stubSources struct {
	goos, goarch string
	numCPU       int
	files        map[string][]byte
	bins         map[string]bool
	runOut       map[string][]byte // key: "name arg arg..."
}

func (s stubSources) GOOS() string   { return s.goos }
func (s stubSources) GOARCH() string { return s.goarch }
func (s stubSources) NumCPU() int    { return s.numCPU }

func (s stubSources) ReadFile(p string) ([]byte, error) {
	if b, ok := s.files[p]; ok {
		return b, nil
	}
	return nil, fmt.Errorf("stub: no file %s", p)
}

func (s stubSources) LookPath(n string) (string, error) {
	if s.bins[n] {
		return "/usr/bin/" + n, nil
	}
	return "", fmt.Errorf("stub: %s not found", n)
}

func (s stubSources) Run(n string, a ...string) ([]byte, error) {
	key := n
	for _, x := range a {
		key += " " + x
	}
	if b, ok := s.runOut[key]; ok {
		return b, nil
	}
	return nil, fmt.Errorf("stub: no run output for %q", key)
}

const ryzenCPUInfo = `processor	: 0
physical id	: 0
core id		: 0

processor	: 1
physical id	: 0
core id		: 0

processor	: 2
physical id	: 0
core id		: 1

processor	: 3
physical id	: 0
core id		: 1
`

// TestDetectLinuxParsesProcFixtures → INSTALL: detection parses /proc-style
// fixtures into HostCaps without touching the real host.
func TestDetectLinuxParsesProcFixtures(t *testing.T) {
	s := stubSources{
		goos:   "linux",
		goarch: "amd64",
		numCPU: 4,
		files: map[string][]byte{
			"/proc/meminfo": []byte("MemTotal:       65802256 kB\nMemFree: 100 kB\n"),
			"/proc/cpuinfo": []byte(ryzenCPUInfo),
		},
		bins: map[string]bool{}, // no GPU tooling -> AccelNone
	}
	caps := DetectWith(s)
	if caps.OS != "linux" || caps.Arch != "amd64" {
		t.Fatalf("os/arch = %s/%s", caps.OS, caps.Arch)
	}
	if caps.LogicalCPU != 4 {
		t.Errorf("logical cpu = %d, want 4", caps.LogicalCPU)
	}
	if caps.PhysicalCPU != 2 {
		t.Errorf("physical cpu = %d, want 2 (two distinct core ids)", caps.PhysicalCPU)
	}
	// 65802256 kB * 1024 = 67381510144 bytes -> 62 GiB floored.
	if got := caps.TotalRAMGB(); got != 62 {
		t.Errorf("ram = %d GiB, want 62", got)
	}
	if caps.Accel != AccelNone {
		t.Errorf("accel = %s, want none", caps.Accel)
	}
}

// TestDetectLinuxAcceleratorProbe → INSTALL: the linux accelerator probe reports
// NVIDIA/ROCm/none from binary presence only.
func TestDetectLinuxAcceleratorProbe(t *testing.T) {
	base := func(bins map[string]bool) stubSources {
		return stubSources{
			goos: "linux", goarch: "amd64", numCPU: 8,
			files: map[string][]byte{
				"/proc/meminfo": []byte("MemTotal: 32000000 kB\n"),
				"/proc/cpuinfo": []byte(ryzenCPUInfo),
			},
			bins: bins,
		}
	}
	cases := []struct {
		name string
		bins map[string]bool
		want Accelerator
	}{
		{"nvidia", map[string]bool{"nvidia-smi": true}, AccelNVIDIA},
		{"rocm", map[string]bool{"rocminfo": true}, AccelROCm},
		{"none", map[string]bool{}, AccelNone},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if got := DetectWith(base(tc.bins)).Accel; got != tc.want {
				t.Errorf("accel = %s, want %s", got, tc.want)
			}
		})
	}
}

// TestDetectDarwinUsesSysctl → INSTALL: darwin detection reads sysctl-style
// values and reports Metal.
func TestDetectDarwinUsesSysctl(t *testing.T) {
	s := stubSources{
		goos:   "darwin",
		goarch: "arm64",
		numCPU: 16,
		runOut: map[string][]byte{
			"sysctl -n hw.memsize":     []byte("137438953472\n"), // 128 GiB
			"sysctl -n hw.physicalcpu": []byte("12\n"),
		},
	}
	caps := DetectWith(s)
	if caps.OS != "darwin" || caps.Arch != "arm64" {
		t.Fatalf("os/arch = %s/%s", caps.OS, caps.Arch)
	}
	if caps.PhysicalCPU != 12 {
		t.Errorf("physical cpu = %d, want 12", caps.PhysicalCPU)
	}
	if got := caps.TotalRAMGB(); got != 128 {
		t.Errorf("ram = %d GiB, want 128", got)
	}
	if caps.Accel != AccelMetal {
		t.Errorf("accel = %s, want apple-metal", caps.Accel)
	}
}

// TestDetectFallsBackOnMissingProbes → INSTALL: a partial probe degrades to safe
// fallbacks (physical->logical, ram 0) instead of erroring.
func TestDetectFallsBackOnMissingProbes(t *testing.T) {
	s := stubSources{goos: "linux", goarch: "arm64", numCPU: 4} // no files, no bins
	caps := DetectWith(s)
	if caps.PhysicalCPU != 4 {
		t.Errorf("physical cpu fallback = %d, want 4 (logical)", caps.PhysicalCPU)
	}
	if caps.TotalRAMBytes != 0 {
		t.Errorf("ram = %d, want 0 on missing meminfo", caps.TotalRAMBytes)
	}
	if caps.Accel != AccelNone {
		t.Errorf("accel = %s, want none", caps.Accel)
	}
}

// TestParseMemTotalKB → INSTALL: the pure meminfo parser extracts MemTotal kB.
func TestParseMemTotalKB(t *testing.T) {
	if got := parseMemTotalKB([]byte("MemFree: 1 kB\nMemTotal:   123456 kB\n")); got != 123456 {
		t.Errorf("parseMemTotalKB = %d, want 123456", got)
	}
	if got := parseMemTotalKB([]byte("no memtotal here\n")); got != 0 {
		t.Errorf("parseMemTotalKB on absent field = %d, want 0", got)
	}
}

// TestParsePhysicalCoresFallsBackToProcessors → INSTALL: when topology fields are
// absent the parser falls back to counting processors.
func TestParsePhysicalCoresFallsBackToProcessors(t *testing.T) {
	noTopo := "processor\t: 0\n\nprocessor\t: 1\n\nprocessor\t: 2\n"
	if got := parsePhysicalCores([]byte(noTopo)); got != 3 {
		t.Errorf("parsePhysicalCores fallback = %d, want 3", got)
	}
	if got := parsePhysicalCores([]byte(ryzenCPUInfo)); got != 2 {
		t.Errorf("parsePhysicalCores = %d, want 2", got)
	}
}
