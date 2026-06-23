package rtmx

import (
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

// writeFixture creates a temp .rtmx database and returns the database path.
func writeFixture(t *testing.T) string {
	t.Helper()
	root := t.TempDir()
	dir := filepath.Join(root, ".rtmx")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	cfg := "rtmx:\n  database: .rtmx/database.csv\n  schema: core\n"
	if err := os.WriteFile(filepath.Join(dir, "config.yaml"), []byte(cfg), 0o644); err != nil {
		t.Fatal(err)
	}
	header := "req_id,category,subcategory,requirement_text,target_value,test_module,test_function,validation_method,status,priority,phase,notes,effort_weeks,dependencies,blocks,assignee,sprint,started_date,completed_date,requirement_file,external_id\n"
	rows := "" +
		"REQ-AAA-001,AAA,X,first open,crit,pkg,TestA,Unit Test,OPEN,HIGH,1,,0.5,,,,,,,,\n" +
		"REQ-EEE-001,EEE,X,second open,crit,pkg,TestE,Unit Test,OPEN,HIGH,1,,0.5,,,,,,,,\n" +
		"REQ-AAA-002,AAA,X,blocked by 001,crit,pkg,TestA2,Unit Test,OPEN,HIGH,1,,0.5,REQ-AAA-001,,,,,,,\n" +
		"REQ-BBB-001,BBB,X,done,crit,pkg,TestB,Unit Test,COMPLETE,HIGH,1,,0.5,,,,,,,,\n" +
		"REQ-CCC-001,CCC,X,blocked,crit,pkg,TestC,Unit Test,BLOCKED,HIGH,1,,0.5,,,,,,,,\n" +
		"REQ-DDD-001,DDD,X,proposed,crit,pkg,TestD,Unit Test,PROPOSED,HIGH,1,,0.5,,,,,,,,\n"
	if err := os.WriteFile(filepath.Join(dir, "database.csv"), []byte(header+rows), 0o644); err != nil {
		t.Fatal(err)
	}
	return filepath.Join(dir, "database.csv")
}

// TestClientStatusMapping → REQ-RTMX-006: lifecycle statuses are mapped and Next
// skips closed/blocked/proposed, honoring dependency readiness.
func TestClientStatusMapping(t *testing.T) {
	db := writeFixture(t)
	c := NewCLIClient(db)
	ctx := context.Background()

	reqs, err := c.store.Requirements()
	if err != nil {
		t.Fatal(err)
	}
	want := map[string]Status{
		"REQ-BBB-001": StatusClosed, "REQ-CCC-001": StatusBlocked, "REQ-DDD-001": StatusProposed,
		"REQ-AAA-001": StatusOpen,
	}
	got := map[string]Status{}
	for _, r := range reqs {
		got[r.ID] = r.Status
	}
	for id, st := range want {
		if got[id] != st {
			t.Errorf("%s: want status %q, got %q", id, st, got[id])
		}
	}

	// First claimable is the first open req with satisfied deps.
	n, err := c.Next(ctx)
	if err != nil || n == nil || n.ID != "REQ-AAA-001" {
		t.Fatalf("Next: want REQ-AAA-001, got %v (err %v)", n, err)
	}
	// AAA-002 is dep-blocked until AAA-001 closes; closing it unblocks AAA-002.
	if err := c.WriteStatus(ctx, "REQ-AAA-001", StatusClosed); err != nil {
		t.Fatal(err)
	}
	if err := c.WriteStatus(ctx, "REQ-EEE-001", StatusClosed); err != nil {
		t.Fatal(err)
	}
	n, err = c.Next(ctx)
	if err != nil || n == nil || n.ID != "REQ-AAA-002" {
		t.Fatalf("Next after closing deps: want REQ-AAA-002, got %v (err %v)", n, err)
	}
}

// TestCLIClientFallback → REQ-RTMX-005: the CSV/CLI client implements the full
// Client interface — claim is atomic (no double-claim), Next skips claimed work,
// verify is delegated, status writes back.
func TestCLIClientFallback(t *testing.T) {
	db := writeFixture(t)
	verified := map[string]bool{"REQ-AAA-001": true}
	c := NewCLIClient(db, WithVerifyFunc(func(_ context.Context, id string) (bool, error) {
		return verified[id], nil
	}))
	c.health = func(context.Context) error { return nil } // avoid shelling rtmx in unit test
	ctx := context.Background()

	if err := c.Claim(ctx, "REQ-AAA-001"); err != nil {
		t.Fatalf("first claim: %v", err)
	}
	if err := c.Claim(ctx, "REQ-AAA-001"); err == nil {
		t.Error("double-claim must fail")
	}
	// Next skips the claimed req and returns the other open one.
	n, err := c.Next(ctx)
	if err != nil || n == nil || n.ID != "REQ-EEE-001" {
		t.Fatalf("Next must skip claimed: want REQ-EEE-001, got %v (err %v)", n, err)
	}
	ok, err := c.Verify(ctx, "REQ-AAA-001")
	if err != nil || !ok {
		t.Fatalf("Verify: want true, got %v (err %v)", ok, err)
	}
	if err := c.WriteStatus(ctx, "REQ-AAA-001", StatusClosed); err != nil {
		t.Fatal(err)
	}
	if err := c.Release(ctx, "REQ-AAA-001"); err != nil {
		t.Fatal(err)
	}
	if err := c.Health(ctx); err != nil {
		t.Fatalf("Health: %v", err)
	}
	// Re-read confirms the writeback persisted.
	r, err := c.store.ByID("REQ-AAA-001")
	if err != nil || r.Status != StatusClosed {
		t.Fatalf("writeback not persisted: %v (err %v)", r, err)
	}
}

// TestMCPClientRoundTrip → REQ-RTMX-004: the MCP stdio client round-trips
// next/claim/release/health against a live `rtmx mcp-server --stdio`.
func TestMCPClientRoundTrip(t *testing.T) {
	if _, err := exec.LookPath("rtmx"); err != nil {
		t.Skip("rtmx binary not on PATH; skipping MCP round-trip")
	}
	db := writeFixture(t)
	ctx := context.Background()
	c, err := DialMCP(ctx, db)
	if err != nil {
		t.Fatalf("DialMCP: %v", err)
	}
	defer c.Close()

	if err := c.Health(ctx); err != nil {
		t.Errorf("Health: %v", err)
	}
	n, err := c.Next(ctx)
	if err != nil {
		t.Fatalf("Next: %v", err)
	}
	if n == nil || n.Status != StatusOpen {
		t.Fatalf("Next: want an open requirement, got %v", n)
	}
	if err := c.Claim(ctx, n.ID); err != nil {
		t.Fatalf("Claim %s: %v", n.ID, err)
	}
	if err := c.Claim(ctx, n.ID); err == nil {
		t.Errorf("double-claim of %s must fail", n.ID)
	}
	if err := c.Release(ctx, n.ID); err != nil {
		t.Errorf("Release %s: %v", n.ID, err)
	}
}
