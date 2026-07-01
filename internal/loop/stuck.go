package loop

// Step is one action->observation pair in an agent trajectory: a tool call and
// its result, or a plain agent message with no tool call (a "monologue" step).
// Ids and timestamps are intentionally excluded — the detector compares semantic
// content only, so a repeat with fresh ids still matches (LONGRUN-009).
type Step struct {
	// Tool is the action's tool name; "" for a plain agent message.
	Tool string
	// Args is the normalized action content/arguments.
	Args string
	// Obs is the observation/result content; "" when the action produced none.
	Obs string
	// Err reports that the observation was an error.
	Err bool
	// Kind is an optional tag, e.g. "condense" for a context-condensation step.
	Kind string
}

// StuckReason identifies why a trajectory is judged stuck ("" = not stuck).
type StuckReason string

// Stuck patterns, mirroring OpenHands' StuckDetector.
const (
	NotStuck            StuckReason = ""
	StuckRepeatedAction StuckReason = "repeated-action"
	StuckRepeatedError  StuckReason = "repeated-error"
	StuckMonologue      StuckReason = "monologue"
	StuckPingPong       StuckReason = "ping-pong"
	StuckCondensing     StuckReason = "repeated-condensation"
)

// StuckThresholds tunes DetectStuck. Defaults mirror OpenHands' StuckDetector.
type StuckThresholds struct {
	RepeatedAction int // identical action->obs pairs in a row (default 4)
	RepeatedError  int // identical actions each erroring (default 3)
	Monologue      int // consecutive no-tool agent messages (default 3)
	PingPongCycles int // A,B alternations, i.e. 2*N steps (default 3)
	Condensations  int // context-condensation steps total (default 10)
}

// DefaultStuckThresholds returns the OpenHands-derived defaults.
func DefaultStuckThresholds() StuckThresholds {
	return StuckThresholds{RepeatedAction: 4, RepeatedError: 3, Monologue: 3, PingPongCycles: 3, Condensations: 10}
}

// DetectStuck reports whether the tail of an agent trajectory shows a stuck
// pattern, and which — repetition, repeated errors, monologue, ping-pong, or
// runaway condensation. It is pure and model-free (LONGRUN-009): the loop can
// run it every turn to park a spinning agent the failure-only circuit breaker
// would miss — the agent isn't "failing", it's looping.
func DetectStuck(steps []Step, t StuckThresholds) StuckReason {
	key := func(s Step) string { return s.Tool + "\x00" + s.Args + "\x00" + s.Obs }
	act := func(s Step) string { return s.Tool + "\x00" + s.Args }

	// Runaway condensation: too many compaction steps overall.
	if t.Condensations > 0 {
		n := 0
		for _, s := range steps {
			if s.Kind == "condense" {
				n++
			}
		}
		if n >= t.Condensations {
			return StuckCondensing
		}
	}
	// Repeated identical erroring actions (observation content may differ).
	if n := t.RepeatedError; n > 0 && len(steps) >= n {
		last := steps[len(steps)-n:]
		k0, same := act(last[0]), true
		for _, s := range last {
			if !s.Err || act(s) != k0 {
				same = false
				break
			}
		}
		if same {
			return StuckRepeatedError
		}
	}
	// Repeated identical action->observation pairs.
	if n := t.RepeatedAction; n > 0 && len(steps) >= n {
		last := steps[len(steps)-n:]
		k0, same := key(last[0]), last[0].Kind != "condense"
		for _, s := range last[1:] {
			if key(s) != k0 {
				same = false
				break
			}
		}
		if same {
			return StuckRepeatedAction
		}
	}
	// Two-state ping-pong: A,B,A,B,... over the last 2*cycles steps.
	if c := t.PingPongCycles; c >= 2 && len(steps) >= 2*c {
		last := steps[len(steps)-2*c:]
		ka, kb := key(last[0]), key(last[1])
		if ka != kb {
			ok := true
			for i, s := range last {
				want := ka
				if i%2 == 1 {
					want = kb
				}
				if key(s) != want {
					ok = false
					break
				}
			}
			if ok {
				return StuckPingPong
			}
		}
	}
	// Monologue: consecutive agent messages with no tool call.
	if n := t.Monologue; n > 0 && len(steps) >= n {
		last := steps[len(steps)-n:]
		mono := true
		for _, s := range last {
			if s.Tool != "" {
				mono = false
				break
			}
		}
		if mono {
			return StuckMonologue
		}
	}
	return NotStuck
}
