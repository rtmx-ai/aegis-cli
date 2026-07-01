// Package loop is the control loop that wires rtmx, the harness, audit logging
// and metrics together: next → drive → verify → retry → escalate.
//
// It supports a single iteration (--once) or a continuous drain until the
// backlog is empty or a stop condition fires. Failure handling is unattended-
// safe: escalation parks the requirement (mark blocked, log, release, continue)
// rather than blocking on a human, a circuit breaker halts after M consecutive
// failures, and a run budget caps requirements and wall-clock. Verify never
// runs concurrently with generation — the loop separates the phases in time.
package loop

import (
	"context"
	"errors"
	"fmt"
	"time"

	"github.com/rtmx-ai/aegis-cli/internal/audit"
	"github.com/rtmx-ai/aegis-cli/internal/config"
	"github.com/rtmx-ai/aegis-cli/internal/harness"
	"github.com/rtmx-ai/aegis-cli/internal/metrics"
	"github.com/rtmx-ai/aegis-cli/internal/rtmx"
)

// Deps are the loop's injected collaborators. Tests supply fakes.
type Deps struct {
	// RTMX is the requirements engine client.
	RTMX rtmx.Client
	// Harness is the coding-agent adapter.
	Harness harness.Adapter
	// Audit is the append-only audit log.
	Audit *audit.Log
	// Metrics collects per-run metrics.
	Metrics *metrics.Collector
	// Now returns the current time; nil uses time.Now. Tests override it.
	Now func() time.Time
}

// Loop runs requirements according to a Config using injected Deps.
type Loop struct {
	cfg  config.Config
	deps Deps
}

// New constructs a Loop. It returns an error if a required dependency is nil.
func New(cfg config.Config, deps Deps) (*Loop, error) {
	if deps.RTMX == nil {
		return nil, errors.New("loop: rtmx client is required")
	}
	if deps.Harness == nil {
		return nil, errors.New("loop: harness adapter is required")
	}
	if deps.Now == nil {
		deps.Now = time.Now
	}
	return &Loop{cfg: cfg, deps: deps}, nil
}

// Outcome is the result of working one requirement.
type Outcome int

// Possible per-requirement outcomes.
const (
	// OutcomeClosed means verify passed and the requirement was closed.
	OutcomeClosed Outcome = iota
	// OutcomeParked means retries were exhausted and the requirement was
	// parked (marked blocked, logged, released) instead of waiting on a human.
	OutcomeParked
	// OutcomeError means an unrecoverable error working the requirement.
	OutcomeError
)

// Result summarizes a full Run.
type Result struct {
	// Attempted is the number of requirements attempted.
	Attempted int
	// Closed is the number closed by verify.
	Closed int
	// Parked is the number parked on escalation.
	Parked int
	// BreakerTripped is true if the circuit breaker halted the run.
	BreakerTripped bool
	// BudgetExhausted is true if the run budget halted the run.
	BudgetExhausted bool
	// Stuck is the number parked because the agent was detected looping
	// (LONGRUN-009), distinct from retry-exhausted parks.
	Stuck int
}

// Run drains the backlog until it is empty or a stop condition fires. When
// once is true it works at most one requirement. Stop conditions are the
// circuit breaker (BreakAfter consecutive failures) and the run budget
// (MaxRequirements and WallClock).
func (l *Loop) Run(ctx context.Context, once bool) (Result, error) {
	var res Result
	start := l.deps.Now()
	consecutiveFailures := 0

	for {
		// Budget: max requirements.
		if l.cfg.Budget.MaxRequirements > 0 && res.Attempted >= l.cfg.Budget.MaxRequirements {
			res.BudgetExhausted = true
			return res, nil
		}
		// Budget: wall clock.
		if l.cfg.Budget.WallClock > 0 && l.deps.Now().Sub(start) >= l.cfg.Budget.WallClock {
			res.BudgetExhausted = true
			return res, nil
		}
		// Honor cancellation.
		if err := ctx.Err(); err != nil {
			return res, err
		}

		req, err := l.deps.RTMX.Next(ctx)
		if err != nil {
			return res, fmt.Errorf("loop: next: %w", err)
		}
		if req == nil {
			// Backlog empty.
			return res, nil
		}

		outcome, stuck, err := l.work(ctx, req)
		if err != nil {
			return res, err
		}
		res.Attempted++

		switch outcome {
		case OutcomeClosed:
			res.Closed++
			consecutiveFailures = 0
		case OutcomeParked:
			res.Parked++
			consecutiveFailures++
			if stuck != NotStuck {
				res.Stuck++
			}
		default:
			consecutiveFailures++
		}

		// Circuit breaker.
		if consecutiveFailures >= l.cfg.BreakAfter {
			res.BreakerTripped = true
			return res, nil
		}

		if once {
			return res, nil
		}
	}
}

// work runs a single requirement: claim → (drive → verify) up to N+1 attempts
// → close or park. Generation and verification are sequenced so they never run
// concurrently on the memory bus. The claim is released on every exit path,
// making the loop resumable after interruption.
func (l *Loop) work(ctx context.Context, req *rtmx.Requirement) (Outcome, StuckReason, error) {
	if err := l.deps.RTMX.Claim(ctx, req.ID); err != nil {
		return OutcomeError, NotStuck, fmt.Errorf("loop: claim %s: %w", req.ID, err)
	}
	l.record(audit.Entry{Action: audit.ActionClaim, RequirementID: req.ID, MachineAuthored: true})

	att := metrics.Attempt{RequirementID: req.ID}
	start := l.deps.Now()

	var closed bool
	var trace []Step
	var stuck StuckReason
	var feedback string // LONGRUN-001: prior attempt's verify output, fed into the next drive
	attempts := l.cfg.Retries + 1
	for i := 0; i < attempts; i++ {
		// Generation phase.
		diff, derr := l.deps.Harness.Drive(ctx, req, feedback)
		att.Turns += diff.Turns
		att.ToolCalls += diff.ToolCalls
		att.ValidToolCalls += diff.ValidToolCalls
		att.Tokens += diff.Tokens
		// LONGRUN-009 (live): stop a spinning agent before verifying or burning
		// the remaining retries — the failure-only breaker would miss a loop.
		trace = append(trace, stepsFromTrace(diff.Trace)...)
		if stuck = DetectStuck(trace, DefaultStuckThresholds()); stuck != NotStuck {
			break
		}
		if derr != nil {
			// Drive failed this attempt; retry if budget remains.
			continue
		}

		// Verify phase — strictly after generation, never concurrent.
		ok, out, verr := l.deps.RTMX.Verify(ctx, req.ID)
		l.record(audit.Entry{
			Action:          audit.ActionVerify,
			RequirementID:   req.ID,
			Result:          verifyResult(ok, verr),
			MachineAuthored: true,
		})
		if verr == nil && ok {
			closed = true
			att.FirstPass = i == 0
			break
		}
		// LONGRUN-001: feed the failed test output into the next drive so the
		// agent fixes the actual failure (run -> inspect -> fix).
		if verr == nil {
			feedback = out
		}
	}

	att.WallClock = l.deps.Now().Sub(start)

	if closed {
		att.Closed = true
		if err := l.deps.RTMX.WriteStatus(ctx, req.ID, rtmx.StatusClosed); err != nil {
			return OutcomeError, NotStuck, fmt.Errorf("loop: write closed %s: %w", req.ID, err)
		}
		_ = l.deps.RTMX.Release(ctx, req.ID)
		l.record(audit.Entry{Action: audit.ActionRelease, RequirementID: req.ID, Result: "closed", MachineAuthored: true})
		l.collect(att)
		return OutcomeClosed, NotStuck, nil
	}

	// Escalation: unattended, park rather than wait.
	att.Escalated = true
	if err := l.deps.RTMX.WriteStatus(ctx, req.ID, rtmx.StatusBlocked); err != nil {
		return OutcomeError, NotStuck, fmt.Errorf("loop: write blocked %s: %w", req.ID, err)
	}
	detail := "retries exhausted; parked unattended"
	if stuck != NotStuck {
		detail = "stuck (" + string(stuck) + "); parked unattended"
	}
	l.record(audit.Entry{
		Action:          audit.ActionEscalate,
		RequirementID:   req.ID,
		Result:          "blocked",
		MachineAuthored: true,
		Detail:          detail,
	})
	_ = l.deps.RTMX.Release(ctx, req.ID)
	l.record(audit.Entry{Action: audit.ActionPark, RequirementID: req.ID, Result: "blocked", MachineAuthored: true})
	l.collect(att)
	return OutcomeParked, stuck, nil
}

// record writes an audit entry if an audit log is configured.
func (l *Loop) record(e audit.Entry) {
	if l.deps.Audit != nil {
		_ = l.deps.Audit.Record(e)
	}
}

// collect records a metrics attempt if a collector is configured.
func (l *Loop) collect(a metrics.Attempt) {
	if l.deps.Metrics != nil {
		l.deps.Metrics.Record(a)
	}
}

// verifyResult renders a verify outcome for the audit log.
func verifyResult(ok bool, err error) string {
	if err != nil {
		return "error"
	}
	if ok {
		return "pass"
	}
	return "fail"
}
