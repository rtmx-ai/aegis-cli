package profile

import "testing"

func TestFitCapacityAndThroughput(t *testing.T) {
	p := HostProfile{TotalRAMBytes: 64 << 30, AvailableRAMBytes: 60 << 30, MemBandwidthBps: 20 << 30}
	spec := ModelSpec{ID: "gemma-4-26b-a4b", File: "x-Q4_K_XL.gguf", SizeBytes: 14 << 30, Origin: "US"}
	fit := Fit(spec, p, 16384, DefaultFloors())
	if !fit.FitsCapacity {
		t.Error("a 14 GiB model must fit 60 GiB available")
	}
	if fit.PredictedTokPerSec <= 0 || fit.PredictedTokPerSec > 200 {
		t.Errorf("tok/s should be a sane positive number, got %v", fit.PredictedTokPerSec)
	}
}

func TestFitCapacityFail(t *testing.T) {
	p := HostProfile{TotalRAMBytes: 8 << 30, AvailableRAMBytes: 6 << 30, MemBandwidthBps: 20 << 30}
	spec := ModelSpec{ID: "big-70b", File: "x-Q4_K_M.gguf", SizeBytes: 40 << 30, Origin: "US"}
	fit := Fit(spec, p, 16384, DefaultFloors())
	if fit.FitsCapacity {
		t.Error("a 40 GiB model must NOT fit 6 GiB available")
	}
	if fit.FitsInteractive || fit.FitsUnattended {
		t.Error("capacity failure must fail both mode floors")
	}
}

func TestRecommendOriginAndRanking(t *testing.T) {
	p := HostProfile{TotalRAMBytes: 64 << 30, AvailableRAMBytes: 60 << 30, MemBandwidthBps: 200 << 30}
	specs := []ModelSpec{
		{ID: "small-us-a2b", File: "q4.gguf", SizeBytes: 8 << 30, Origin: "US"},
		{ID: "big-us-a4b", File: "q4.gguf", SizeBytes: 20 << 30, Origin: "US"},
		{ID: "big-cn-a3b", File: "q4.gguf", SizeBytes: 30 << 30, Origin: "CN"},
	}
	allowed := func(o string) bool { return o == "US" }
	rec := Recommend(specs, allowed, p, 16384, DefaultFloors())
	for _, f := range rec.Fits {
		if f.ID == "big-cn-a3b" {
			t.Error("a CN-origin model must be excluded under US-only policy")
		}
	}
	if rec.Interactive != "big-us-a4b" {
		t.Errorf("interactive pick = %q, want big-us-a4b (largest US that clears the floor)", rec.Interactive)
	}
}

func TestProbeSmoke(t *testing.T) {
	p := Probe()
	if p.MemBandwidthBps == 0 {
		t.Error("bandwidth probe returned 0")
	}
	if p.TotalRAMBytes > 0 && p.AvailableRAMBytes > p.TotalRAMBytes {
		t.Error("available RAM must not exceed total")
	}
}

// TestFitUsesActiveParams: an explicit catalog active_params (MoE) overrides the dense estimate that
// an id without an `aNb` hint would otherwise get — predicting a realistically faster decode rate.
func TestFitUsesActiveParams(t *testing.T) {
	p := HostProfile{TotalRAMBytes: 64 << 30, AvailableRAMBytes: 60 << 30, MemBandwidthBps: 20 << 30}
	dense := ModelSpec{ID: "moe-35b", File: "x-Q4_K_M.gguf", SizeBytes: 20 << 30, Origin: "US"}
	moe := dense
	moe.ActiveParams = 3_000_000_000
	fd := Fit(dense, p, 16384, DefaultFloors())
	fm := Fit(moe, p, 16384, DefaultFloors())
	if fm.PredictedTokPerSec <= fd.PredictedTokPerSec {
		t.Errorf("explicit active_params must predict faster than dense: moe=%.1f dense=%.1f", fm.PredictedTokPerSec, fd.PredictedTokPerSec)
	}
}
