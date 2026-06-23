// Package audit provides an append-only, in-enclave audit log.
//
// Every claim and verify the loop performs emits one immutable line recording
// who/what/when plus a provenance flag marking machine-authored actions. The log
// is a local file only; nothing here makes a network call.
package audit

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"sync"
	"time"
)

// Action names the kind of event recorded.
type Action string

// Recorded action kinds.
const (
	ActionClaim    Action = "claim"
	ActionRelease  Action = "release"
	ActionVerify   Action = "verify"
	ActionEscalate Action = "escalate"
	ActionPark     Action = "park"
	ActionPropose  Action = "propose"
)

// Entry is a single immutable audit record.
type Entry struct {
	// Time is when the action occurred (UTC).
	Time time.Time `json:"time"`
	// Actor identifies who performed the action (e.g. "aegis-loop").
	Actor string `json:"actor"`
	// Action is the kind of event.
	Action Action `json:"action"`
	// RequirementID is the affected requirement, if any.
	RequirementID string `json:"requirement_id,omitempty"`
	// Result is a free-form outcome (e.g. "pass", "fail", "blocked").
	Result string `json:"result,omitempty"`
	// MachineAuthored flags actions authored by the machine rather than a human.
	MachineAuthored bool `json:"machine_authored"`
	// Detail is optional human-readable context.
	Detail string `json:"detail,omitempty"`
}

// Log is an append-only audit log backed by an io.Writer. It is safe for
// concurrent use.
type Log struct {
	mu     sync.Mutex
	w      io.Writer
	closer io.Closer
	actor  string
}

// New returns a Log writing to w. The actor is stamped on every entry.
func New(w io.Writer, actor string) *Log {
	return &Log{w: w, actor: actor}
}

// Open opens (creating if needed) an append-only log file at path.
func Open(path, actor string) (*Log, error) {
	f, err := os.OpenFile(path, os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0o600)
	if err != nil {
		return nil, fmt.Errorf("audit: open %s: %w", path, err)
	}
	return &Log{w: f, closer: f, actor: actor}, nil
}

// Record appends one entry. Time and Actor are filled if unset.
func (l *Log) Record(e Entry) error {
	l.mu.Lock()
	defer l.mu.Unlock()
	if e.Time.IsZero() {
		e.Time = time.Now().UTC()
	}
	if e.Actor == "" {
		e.Actor = l.actor
	}
	line, err := json.Marshal(e)
	if err != nil {
		return fmt.Errorf("audit: marshal entry: %w", err)
	}
	if _, err := l.w.Write(append(line, '\n')); err != nil {
		return fmt.Errorf("audit: write entry: %w", err)
	}
	return nil
}

// Close closes the underlying file if Log owns one.
func (l *Log) Close() error {
	if l.closer != nil {
		return l.closer.Close()
	}
	return nil
}
