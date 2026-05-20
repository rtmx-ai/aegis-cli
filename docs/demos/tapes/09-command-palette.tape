# Command palette: slash commands
#
# Expected outcome:
#   1. Launch chat
#   2. /context shows current file context (likely empty)
#   3. /add src/lib.rs adds a file to context
#   4. /trivia shows a random defense AI historical fact
#   5. /clear resets the session
#   6. /help shows all available commands
#   7. /quit exits cleanly
#
# Covers: REQ-TUI-010 (slash commands), REQ-TUI-113 (trivia)

Output docs/demos/gifs/09-command-palette.gif
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

# Show context
Type@0ms "/context"
Escape
Sleep 200ms
Enter
Sleep 3s

# Add a file to context
Type@0ms "/add src/lib.rs"
Escape
Sleep 200ms
Enter
Sleep 2s

# Show a defense AI trivia fact
Type@0ms "/trivia"
Escape
Sleep 200ms
Enter
Sleep 4s

# Clear the session
Type@0ms "/clear"
Escape
Sleep 200ms
Enter
Sleep 2s

# Show help
Type@0ms "/help"
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
