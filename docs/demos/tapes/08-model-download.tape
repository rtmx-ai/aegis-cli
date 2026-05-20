# Model download: pull models from Ollama
#
# Expected outcome:
#   1. Launch chat
#   2. /model shows current model
#   3. /model download gemma3:4b triggers download (or shows already present)
#   4. /doctor shows system health
#   5. /quit exits cleanly
#
# Covers: REQ-TUI-108 (download command), REQ-TUI-109 (progress bar)

Output docs/demos/gifs/08-model-download.gif
Set Width 1200
Set Height 700
Set FontSize 14
Set Theme "Catppuccin Mocha"
Set TypingSpeed 40ms
Set Padding 20

# Clear stale session
Hide
Type "rm -f ~/.aegis/sessions/current.json"
Enter
Sleep 500ms
Show

Type "aegis chat"
Enter
Sleep 5s

# Show the current model
Type@0ms "/model"
Escape
Sleep 200ms
Enter
Sleep 2s

# Attempt to download (shows in-progress or completion)
Type@0ms "/model download gemma3:4b"
Escape
Sleep 200ms
Enter
Sleep 8s

# Show doctor to check health
Type@0ms "/doctor"
Escape
Sleep 200ms
Enter
Sleep 4s

# Exit
Type@0ms "/quit"
Escape
Sleep 200ms
Enter
Sleep 1s
