package opencode

import (
	"fmt"
	"io"
	"os"
	"sync"
	"time"
)

// LaunchSplash writes the immediate "Loading aegis…" splash the instant the TUI command runs, so bare
// `aegis` shows activity right away instead of a blank terminal during the model load + repo-map staging
// that otherwise leave a dead pause (OC-048). It returns a stop func to call once the model is up, just
// before OpenCode paints its own TUI.
//
// When w is a real terminal (a character device) it also animates a spinner on the status line; the stop
// func halts the animation and clears the line. When w is not a TTY (a pipe or a test buffer) it is a
// one-shot banner and stop is a no-op. The returned stop is idempotent and safe to call from any exit path.
func LaunchSplash(w io.Writer) func() {
	// The static banner prints unconditionally (plain text — no ANSI — so it is clean even when captured):
	// this is the instant feedback the operator sees the moment `aegis` starts.
	fmt.Fprint(w, "\n  aegis — air-gapped agentic coding\n")
	fmt.Fprint(w, "  Loading aegis… bringing up the local model (first launch loads the weights — one moment)\n")

	f, ok := w.(*os.File)
	if !ok {
		return func() {}
	}
	fi, err := f.Stat()
	if err != nil || fi.Mode()&os.ModeCharDevice == 0 {
		return func() {} // not a terminal — leave it at the one-shot banner, no animation
	}

	done := make(chan struct{})
	stopped := make(chan struct{})
	go func() {
		defer close(stopped)
		frames := []rune("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
		t := time.NewTicker(100 * time.Millisecond)
		defer t.Stop()
		for i := 0; ; i++ {
			select {
			case <-done:
				fmt.Fprint(f, "\r\033[2K") // carriage-return + clear-line, so OpenCode paints clean
				return
			case <-t.C:
				fmt.Fprintf(f, "\r  %c working…", frames[i%len(frames)])
			}
		}
	}()
	var once sync.Once
	return func() { once.Do(func() { close(done); <-stopped }) }
}
