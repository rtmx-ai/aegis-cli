# Audit ledger: every action logged, metadata only
#
# Expected outcome:
#   1. aegis --version shows the installed version
#   2. aegis history shows past session audit entries
#   3. Launch chat, ask a question to generate audit events
#   4. /cost shows token usage and cost tracking
#   5. /quit exits cleanly

Output docs/demos/gifs/04-audit-ledger.gif
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

# Show aegis version
Type "aegis --version"
Enter
Sleep 2s

# Show history command
Type "aegis history"
Enter
Sleep 4s

# Launch chat session
Type "aegis chat"
Enter
Sleep 5s

# Ask a question to generate audit events
Type "Hello, what can you help with?"
Enter
Sleep 12s

# Show cost tracking
Type@0ms "/cost"
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
