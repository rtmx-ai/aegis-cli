// Package install bootstraps aegis-cli for a host: it detects the machine's
// capabilities, maps them to a recommended serving target, model tier, and
// calibration seed, and writes an offline-safe config the rest of the system
// reads.
//
// Detection is the only part that touches the real host, and it is fully
// injectable: DetectWith takes a Sources interface so tests provide /proc-style
// fixtures and never depend on the CI host's hardware. The pure parsing helpers
// (parseMemTotalKB, parsePhysicalCores) operate on provided bytes and carry the
// real logic.
//
// Nothing here makes a network call; detection reads local files and runs local
// probes only (skills/airgap-hygiene).
package install

import (
	"bufio"
	"bytes"
	"fmt"
	"math"
	"os"
	"os/exec"
	"runtime"
	"strconv"
	"strings"
)

// Accelerator identifies the best-effort detected inference accelerator.
type Accelerator string

// Known accelerators. "none" means CPU-only inference.
const (
	AccelNone   Accelerator = "none"
	AccelMetal  Accelerator = "apple-metal"
	AccelNVIDIA Accelerator = "nvidia-cuda"
	AccelROCm   Accelerator = "amd-rocm"
)

// HostCaps describes a host's inference-relevant capabilities. It is the input
// to Plan and is produced by Detect/DetectWith.
type HostCaps struct {
	// OS is the operating system (runtime.GOOS): "linux", "darwin", ...
	OS string `json:"os"`
	// Arch is the architecture (runtime.GOARCH): "amd64", "arm64", ...
	Arch string `json:"arch"`
	// LogicalCPU is the count of logical CPUs (hyperthreads included).
	LogicalCPU int `json:"logical_cpu"`
	// PhysicalCPU is the count of physical cores; falls back to LogicalCPU
	// when a physical count cannot be determined.
	PhysicalCPU int `json:"physical_cpu"`
	// TotalRAMBytes is total system RAM in bytes (unified memory on darwin).
	TotalRAMBytes uint64 `json:"total_ram_bytes"`
	// Accel is the best-effort detected accelerator.
	Accel Accelerator `json:"accelerator"`
}

// TotalRAMGB returns total RAM in whole GiB (floored), for human-readable
// summaries.
func (h HostCaps) TotalRAMGB() uint64 { return h.TotalRAMBytes / (1 << 30) }

// Sources abstracts the host probes detection needs, so tests inject fixtures
// instead of reading the real machine.
type Sources interface {
	// GOOS returns the operating system identifier (runtime.GOOS).
	GOOS() string
	// GOARCH returns the architecture identifier (runtime.GOARCH).
	GOARCH() string
	// NumCPU returns the logical CPU count (runtime.NumCPU()).
	NumCPU() int
	// ReadFile reads a host file (e.g. /proc/meminfo). Mirrors os.ReadFile.
	ReadFile(path string) ([]byte, error)
	// LookPath reports whether a binary is on PATH. Mirrors exec.LookPath.
	LookPath(name string) (string, error)
	// Run runs a command and returns its combined output. Used for darwin
	// sysctl probes; never used for anything that could egress.
	Run(name string, args ...string) ([]byte, error)
}

// realSources is the production Sources backed by the actual host.
type realSources struct{}

func (realSources) GOOS() string                      { return runtime.GOOS }
func (realSources) GOARCH() string                    { return runtime.GOARCH }
func (realSources) NumCPU() int                       { return runtime.NumCPU() }
func (realSources) ReadFile(p string) ([]byte, error) { return os.ReadFile(p) }
func (realSources) LookPath(n string) (string, error) { return exec.LookPath(n) }
func (realSources) Run(n string, a ...string) ([]byte, error) {
	return exec.Command(n, a...).Output()
}

// Detect probes the real host. It is a thin wrapper over DetectWith.
func Detect() HostCaps { return DetectWith(realSources{}) }

// DetectWith builds HostCaps from the provided Sources. It never errors: every
// probe degrades to a safe, clearly-derived fallback (e.g. physical cores fall
// back to logical, RAM to 0) so a partial probe still yields a usable plan.
func DetectWith(s Sources) HostCaps {
	caps := HostCaps{
		OS:         s.GOOS(),
		Arch:       s.GOARCH(),
		LogicalCPU: s.NumCPU(),
	}
	switch caps.OS {
	case "darwin":
		caps.PhysicalCPU = detectPhysicalDarwin(s, caps.LogicalCPU)
		caps.TotalRAMBytes = detectRAMDarwin(s)
		caps.Accel = detectAccelDarwin(s)
	default: // linux and everything else use the /proc path
		caps.PhysicalCPU = detectPhysicalLinux(s, caps.LogicalCPU)
		caps.TotalRAMBytes = detectRAMLinux(s)
		caps.Accel = detectAccelLinux(s)
	}
	if caps.PhysicalCPU < 1 {
		caps.PhysicalCPU = max1(caps.LogicalCPU)
	}
	return caps
}

// --- linux probes -------------------------------------------------------------

// detectRAMLinux reads MemTotal from /proc/meminfo. Returns 0 on failure.
func detectRAMLinux(s Sources) uint64 {
	data, err := s.ReadFile("/proc/meminfo")
	if err != nil {
		return 0
	}
	return parseMemTotalKB(data) * 1024
}

// detectPhysicalLinux counts distinct physical cores from /proc/cpuinfo by
// pairing "physical id" with "core id". Falls back to the logical count.
func detectPhysicalLinux(s Sources, logical int) int {
	data, err := s.ReadFile("/proc/cpuinfo")
	if err != nil {
		return logical
	}
	if n := parsePhysicalCores(data); n > 0 {
		return n
	}
	return logical
}

// detectAccelLinux probes for an NVIDIA or AMD/ROCm stack by binary presence
// only — no execution that could fetch anything. Order: NVIDIA, then ROCm.
func detectAccelLinux(s Sources) Accelerator {
	if _, err := s.LookPath("nvidia-smi"); err == nil {
		return AccelNVIDIA
	}
	if _, err := s.LookPath("rocminfo"); err == nil {
		return AccelROCm
	}
	return AccelNone
}

// --- darwin probes ------------------------------------------------------------

// detectRAMDarwin reads hw.memsize via sysctl. Returns 0 on failure.
func detectRAMDarwin(s Sources) uint64 {
	out, err := s.Run("sysctl", "-n", "hw.memsize")
	if err != nil {
		return 0
	}
	return parseUint(out)
}

// detectPhysicalDarwin reads hw.physicalcpu via sysctl. Falls back to logical.
func detectPhysicalDarwin(s Sources, logical int) int {
	out, err := s.Run("sysctl", "-n", "hw.physicalcpu")
	if err != nil {
		return logical
	}
	if u := parseUint(out); u > 0 && u <= math.MaxInt32 {
		return int(u) // bounded: a CPU count is well within int32 (gosec G115)
	}
	return logical
}

// detectAccelDarwin reports Metal on Apple Silicon (arm64). On any darwin host
// Metal is the offload path the stack uses, so darwin reports apple-metal.
func detectAccelDarwin(s Sources) Accelerator {
	return AccelMetal
}

// --- pure parsing helpers (carry the real logic; unit-tested directly) --------

// parseMemTotalKB extracts the MemTotal value (in kB) from /proc/meminfo bytes.
// Returns 0 when the field is absent or malformed.
func parseMemTotalKB(data []byte) uint64 {
	sc := bufio.NewScanner(bytes.NewReader(data))
	for sc.Scan() {
		line := sc.Text()
		if !strings.HasPrefix(line, "MemTotal:") {
			continue
		}
		fields := strings.Fields(line)
		// Expect: ["MemTotal:" "<value>" "kB"]
		if len(fields) >= 2 {
			if v, err := strconv.ParseUint(fields[1], 10, 64); err == nil {
				return v
			}
		}
	}
	return 0
}

// parsePhysicalCores counts distinct (physical id, core id) pairs in
// /proc/cpuinfo bytes. When the topology fields are absent (some VMs/ARM), it
// falls back to counting "processor" entries. Returns 0 if neither is present.
func parsePhysicalCores(data []byte) int {
	type coreKey struct{ pkg, core string }
	seen := map[coreKey]struct{}{}
	var curPkg, curCore string
	processors := 0
	have := false

	flush := func() {
		if curPkg != "" && curCore != "" {
			seen[coreKey{curPkg, curCore}] = struct{}{}
			have = true
		}
		curPkg, curCore = "", ""
	}

	sc := bufio.NewScanner(bytes.NewReader(data))
	for sc.Scan() {
		line := sc.Text()
		if strings.TrimSpace(line) == "" {
			flush()
			continue
		}
		key, val := splitColon(line)
		switch key {
		case "processor":
			processors++
		case "physical id":
			curPkg = val
		case "core id":
			curCore = val
		}
	}
	flush()

	if have {
		return len(seen)
	}
	return processors
}

// splitColon splits a "key : value" cpuinfo line into trimmed key and value.
func splitColon(line string) (key, val string) {
	i := strings.IndexByte(line, ':')
	if i < 0 {
		return strings.TrimSpace(line), ""
	}
	return strings.TrimSpace(line[:i]), strings.TrimSpace(line[i+1:])
}

// parseUint parses a single non-negative integer from trimmed bytes, 0 on fail.
func parseUint(b []byte) uint64 {
	v, err := strconv.ParseUint(strings.TrimSpace(string(b)), 10, 64)
	if err != nil {
		return 0
	}
	return v
}

// max1 clamps a count to at least 1.
func max1(n int) int {
	if n < 1 {
		return 1
	}
	return n
}

// String renders HostCaps as a one-line human summary.
func (h HostCaps) String() string {
	return fmt.Sprintf("os=%s arch=%s cpu=%d/%d(physical) ram=%dGiB accel=%s",
		h.OS, h.Arch, h.PhysicalCPU, h.LogicalCPU, h.TotalRAMGB(), h.Accel)
}
