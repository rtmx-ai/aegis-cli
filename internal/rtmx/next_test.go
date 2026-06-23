package rtmx

import (
	"context"
	"testing"
)

func TestNextSkipsProposedAndClosed(t *testing.T) {
	f := NewFake(
		&Requirement{ID: "A-001", Status: StatusClosed},
		&Requirement{ID: "A-002", Status: StatusProposed},
		&Requirement{ID: "A-003", Status: StatusOpen},
	)
	r, err := f.Next(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if r == nil || r.ID != "A-003" {
		t.Fatalf("Next returned %+v, want A-003", r)
	}
}

func TestNextEmptyBacklog(t *testing.T) {
	f := NewFake()
	r, err := f.Next(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if r != nil {
		t.Fatalf("empty backlog should return nil, got %+v", r)
	}
}
