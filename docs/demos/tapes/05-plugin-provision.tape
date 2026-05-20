# Plugin provisioning: infrastructure management
#
# Expected outcome:
#   1. Launch aegis chat
#   2. /infra shows available infrastructure commands
#   3. /doctor shows system health check
#   4. /help shows full command list
#   5. /quit exits cleanly

Output docs/demos/gifs/05-plugin-provision.gif
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

# Show available infrastructure commands
Type@0ms "/infra"
Escape
Sleep 200ms
Enter
Sleep 3s

# Run doctor to check health
Type@0ms "/doctor"
Escape
Sleep 200ms
Enter
Sleep 4s

# Show help for full command list
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
