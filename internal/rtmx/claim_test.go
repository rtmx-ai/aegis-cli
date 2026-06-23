package rtmx

import (
	"context"
	"testing"
)

func TestClaimIsAtomicNoDoubleClaim(t *testing.T) {
	f := NewFake(&Requirement{ID: "A-001", Status: StatusOpen})
	ctx := context.Background()
	if err := f.Claim(ctx, "A-001"); err != nil {
		t.Fatalf("first claim should succeed: %v", err)
	}
	if err := f.Claim(ctx, "A-001"); err == nil {
		t.Fatal("second claim of same id must fail (no double-claim)")
	}
	// A claimed requirement is not returned by Next.
	if r, _ := f.Next(ctx); r != nil {
		t.Fatalf("claimed req must not be Next, got %+v", r)
	}
	// Release frees it for re-claim.
	if err := f.Release(ctx, "A-001"); err != nil {
		t.Fatal(err)
	}
	if err := f.Claim(ctx, "A-001"); err != nil {
		t.Fatalf("re-claim after release should succeed: %v", err)
	}
}
