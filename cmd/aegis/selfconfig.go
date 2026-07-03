package main

import (
	"os"

	"github.com/rtmx-ai/aegis-cli/internal/install"
	"github.com/rtmx-ai/aegis-cli/internal/profile"
	"github.com/rtmx-ai/aegis-cli/internal/serving"
)

// ctxLadder is the standard context windows aegis steps DOWN through to fit the host memory, largest
// first. Only rungs <= the desired ceiling are considered.
var ctxLadder = []int{32768, 24576, 16384, 12288, 8192, 4096}

// serveReserveBytes is the memory aegis holds back from the model+KV budget at serve time: macOS UI +
// the GPU wired-limit slack (Metal caps GPU working set below total unified RAM) + the co-located
// opencode/node and rtmx processes + llama.cpp compute/scratch buffers. It is intentionally larger than
// the profiler's headless reserve (internal/profile) because the interactive serve path co-runs the
// whole harness, not just the model. Tunable as we calibrate real hosts.
const serveReserveBytes uint64 = 6 << 30 // 6 GiB

// fitCtxSize caps a desired context window to the largest standard window that actually FITS this host's
// memory for the given model (SELF-CONFIG, every launch): model weights + estimated KV(ctx) must sit
// within (available RAM − serve reserve). It runs off the LIVE detected RAM + the on-disk model size, so
// a big model on a small box (e.g. a 24B Q4 model on a 24 GB Mac) auto-sizes its context instead of
// serving 32k and OOMing. Returns the desired size unchanged when the model or RAM can't be measured, or
// when the operator pinned AEGIS_CTX_SIZE (an explicit override takes responsibility for the fit).
func fitCtxSize(desired int, modelPath string) int {
	if os.Getenv("AEGIS_CTX_SIZE") != "" {
		return desired // operator pinned it — honor the explicit choice, don't second-guess memory
	}
	fi, err := os.Stat(modelPath)
	if err != nil || fi.Size() <= 0 {
		return desired // can't measure the model — trust the resolver
	}
	avail := install.Detect().TotalRAMBytes // unified on darwin; total is the capacity ceiling
	return fitCtxTokens(desired, uint64(fi.Size()), avail, modelPath)
}

// fitCtxTokens is the pure fit decision (unit-testable): the largest ladder rung <= desired whose
// weights + KV estimate fit within (availRAM − serveReserveBytes). Falls to the smallest rung when
// nothing fits (llama.cpp will still attempt it), and returns desired when RAM/size are unknown.
func fitCtxTokens(desired int, sizeBytes, availRAM uint64, modelPathOrID string) int {
	if desired <= 0 {
		desired = serving.DefaultCtxSize
	}
	if sizeBytes == 0 || availRAM == 0 || availRAM <= serveReserveBytes {
		return desired
	}
	budget := availRAM - serveReserveBytes
	total := profile.EstimateTotalParams(modelPathOrID, modelPathOrID, sizeBytes)
	for _, c := range ctxLadder {
		if c > desired {
			continue
		}
		if sizeBytes+profile.KVCacheBytes(total, c) <= budget {
			return c
		}
	}
	return ctxLadder[len(ctxLadder)-1]
}
