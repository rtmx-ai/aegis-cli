package audit

import (
	"bufio"
	"bytes"
	"encoding/json"
	"strings"
	"testing"
)

func TestLogEmitsImmutableLine(t *testing.T) {
	var buf bytes.Buffer
	l := New(&buf, "aegis-loop")
	if err := l.Record(Entry{Action: ActionClaim, RequirementID: "A-001", MachineAuthored: true}); err != nil {
		t.Fatal(err)
	}
	if err := l.Record(Entry{Action: ActionVerify, RequirementID: "A-001", Result: "pass", MachineAuthored: true}); err != nil {
		t.Fatal(err)
	}

	lines := strings.Split(strings.TrimSpace(buf.String()), "\n")
	if len(lines) != 2 {
		t.Fatalf("want 2 audit lines, got %d", len(lines))
	}
	var e Entry
	if err := json.Unmarshal([]byte(lines[0]), &e); err != nil {
		t.Fatalf("line must be valid JSON: %v", err)
	}
	if e.Actor != "aegis-loop" {
		t.Errorf("actor = %q, want aegis-loop", e.Actor)
	}
	if e.Action != ActionClaim || !e.MachineAuthored {
		t.Errorf("unexpected entry: %+v", e)
	}
	if e.Time.IsZero() {
		t.Error("entry time should be stamped")
	}
}

func TestLogAppendOnly(t *testing.T) {
	var buf bytes.Buffer
	l := New(&buf, "actor")
	for i := 0; i < 5; i++ {
		_ = l.Record(Entry{Action: ActionClaim, RequirementID: "X"})
	}
	count := 0
	s := bufio.NewScanner(&buf)
	for s.Scan() {
		count++
	}
	if count != 5 {
		t.Fatalf("append-only log should have 5 lines, got %d", count)
	}
}
