# Model picker: show and switch models
#
# Expected outcome:
#   1. Launch chat
#   2. /model shows current model name
#   3. /model gemma3:4b switches to that model
#   4. /model confirms the switch took effect
#   5. /quit exits cleanly
#
# Covers: REQ-TUI-025 (model switch)

Output docs/demos/gifs/07-model-picker.gif
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

# Show current model
Type@0ms "/model"
Escape
Sleep 200ms
Enter
Sleep 3s

# Switch to a different model
Type@0ms "/model gemma3:4b"
Escape
Sleep 200ms
Enter
Sleep 3s

# Verify the switch took effect
Type@0ms "/model"
Escape
Sleep 200ms
Enter
Sleep 3s

# Exit
Type@0ms "/quit"
Escape
Sleep 200ms
Enter
Sleep 1s
