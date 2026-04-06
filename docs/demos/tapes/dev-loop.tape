## VHS tape: aegis-cli dev loop demo
## Renders to docs/demos/dev-loop.gif (16:9, 1280x720)
##
## Generates a simulated dev loop session showing:
##   Left pane: Claude Code with critical-path prompt
##   Right pane: aegis TUI running in sandbox
##   Bottom: tmux status bar with keyboard reference
##
## Run: vhs docs/demos/tapes/dev-loop.tape
## Install vhs: brew install vhs

Output docs/demos/dev-loop.gif

Set FontSize 14
Set Width 1280
Set Height 720
Set Theme "Dracula"
Set Padding 20
Set TypingSpeed 30ms

# Show the launch command
Type "./scripts/dev.sh"
Sleep 500ms
Enter

# Wait for the simulated session to render
Sleep 4s

# Hold the final frame
Sleep 2s
