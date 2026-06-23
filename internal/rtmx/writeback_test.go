package rtmx

import (
	"context"
	"testing"
)

func TestWriteStatusWritesBack(t *testing.T) {
	req := &Requirement{ID: "A-001", Status: StatusOpen}
	f := NewFake(req)
	f.VerifyResult["A-001"] = true
	ctx := context.Background()

	ok, err := f.Verify(ctx, "A-001")
	if err != nil || !ok {
		t.Fatalf("verify = (%v,%v), want (true,nil)", ok, err)
	}
	if err := f.WriteStatus(ctx, "A-001", StatusClosed); err != nil {
		t.Fatal(err)
	}
	if req.Status != StatusClosed {
		t.Fatalf("status = %q, want closed", req.Status)
	}
}

func TestWriteStatusUnknownID(t *testing.T) {
	f := NewFake()
	if err := f.WriteStatus(context.Background(), "ghost", StatusClosed); err == nil {
		t.Fatal("writing status to unknown id must error")
	}
}
