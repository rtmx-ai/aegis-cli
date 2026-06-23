package propose

import (
	"context"
	"testing"
)

// TestBoundsChildrenInheritParentTests models PROPOSE-003: children inherit the
// parent's tests and the child cap is enforced.
func TestBoundsChildrenInheritParentTests(t *testing.T) {
	p := New(DefaultBounds(), nil)
	prop, err := p.Propose(context.Background(), parent(), []string{"a", "b"})
	if err != nil {
		t.Fatal(err)
	}
	for _, c := range prop.Children {
		if len(c.Tests) != 1 || c.Tests[0] != "loop/e2e_test" {
			t.Errorf("child %s tests = %v, want inherited [loop/e2e_test]", c.ID, c.Tests)
		}
	}
}

func TestBoundsChildCapEnforced(t *testing.T) {
	p := New(Bounds{MaxDepth: 1, MaxChildren: 2}, nil)
	_, err := p.Propose(context.Background(), parent(), []string{"a", "b", "c"})
	if err == nil {
		t.Fatal("exceeding the child cap must be a hard stop")
	}
}

func TestBoundsDepthMustBePositive(t *testing.T) {
	p := New(Bounds{MaxDepth: 0, MaxChildren: 4}, nil)
	if _, err := p.Propose(context.Background(), parent(), []string{"a"}); err == nil {
		t.Fatal("max depth < 1 must fail")
	}
}
