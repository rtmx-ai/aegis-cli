package main

import (
	"flag"
	"fmt"
	"io"
	"strings"

	"github.com/rtmx-ai/aegis-cli/internal/index"
)

// cmdMap prints a ranked, token-budgeted repo map of the working tree
// (INDEX-001-P05): a model-free skeleton of definition signatures a small local
// model uses to call real symbols without loading whole files. Extra positional
// args personalize the ranking (identifiers/paths). Exposed to the harness via
// the /map opencode command; pure-local, no egress.
func cmdMap(args []string, stdout, stderr io.Writer) int {
	fs := flag.NewFlagSet("map", flag.ContinueOnError)
	fs.SetOutput(stderr)
	budget := fs.Int("budget", 4000, "approximate character budget for the map")
	root := fs.String("root", ".", "repo root to scan")
	if err := fs.Parse(args); err != nil {
		return 2
	}
	out, err := index.Build(index.Options{Root: *root, Mentions: fs.Args(), TokenBudget: *budget})
	if err != nil {
		fmt.Fprintf(stderr, "aegis map: %v\n", err)
		return 1
	}
	if strings.TrimSpace(out) == "" {
		fmt.Fprintln(stdout, "(no Go symbols found)")
		return 0
	}
	fmt.Fprint(stdout, out)
	return 0
}
