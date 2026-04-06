## VHS tape: aegis-cli dev loop demo (REAL recording)
##
## Records an actual launch of ./scripts/dev.sh, capturing both panes:
##   Left:  Claude Code (real session, real prompt response)
##   Right: aegis-cli TUI running in ~/aegis-sandbox
##
## Renders to docs/demos/dev-loop.gif at 1280x720 (16:9).
##
## Prerequisites:
##   - vhs installed (brew install vhs)
##   - claude authenticated (claude --version succeeds)
##   - tmux installed
##   - bacon installed (cargo install bacon)
##   - Run from project root: vhs docs/demos/tapes/dev-loop.tape
##
## Note: this records a REAL session, so it consumes Claude API tokens
## and takes ~90 seconds to complete (build + Claude response + render).

Output docs/demos/dev-loop.gif

Set FontSize 12
Set Width 1280
Set Height 720
Set Theme "Dracula"
Set Padding 16
Set TypingSpeed 40ms

# Show the launch command
Type "./scripts/dev.sh"
Sleep 500ms
Enter

# Wait for tmux to launch and both panes to populate:
#   - git pull (left pane terminal)
#   - sandbox clone update
#   - tmux session start
#   - claude launches with critical-path prompt
#   - dev-run.sh builds aegis (~15-30s)
#   - aegis TUI initializes in right pane
Sleep 60s

# Hold the final frame so the GIF ends on a complete picture
Sleep 3s

# Detach cleanly (does not kill the session)
Ctrl+b
Type "d"
