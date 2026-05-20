# .aegisignore: secrets never reach the LLM
#
# Expected outcome:
#   1. cat .aegisignore shows the ignore rules
#   2. Launch chat, ask about config files
#   3. Agent response respects .aegisignore (sensitive files excluded)
#   4. /context shows active context (filtered)
#   5. /quit exits cleanly

Output docs/demos/gifs/06-aegisignore.gif
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

# Show the aegisignore file
Type "cat .aegisignore"
Enter
Sleep 3s

# Launch chat
Type "aegis chat"
Enter
Sleep 5s

# Ask about configuration -- agent should respect aegisignore
Type "What configuration files exist in this project?"
Enter
Sleep 15s

# Show context
Type@0ms "/context"
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
