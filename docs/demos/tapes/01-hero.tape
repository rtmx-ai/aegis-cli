# Hero demo: 30-second pitch for README top
#
# Expected outcome:
#   1. Launch aegis chat, see splash with logo + trivia
#   2. Ask a natural language question, see streaming LLM response
#   3. /help shows available commands
#   4. /model shows current model
#   5. /quit exits cleanly

Output docs/demos/gifs/01-hero.gif
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

# Ask a simple question to show streaming
Type "What files are in the current directory?"
Enter
Sleep 15s

# Show the help command
Type@0ms "/help"
Escape
Sleep 200ms
Enter
Sleep 4s

# Show current model
Type@0ms "/model"
Escape
Sleep 200ms
Enter
Sleep 3s

# Clean exit
Type@0ms "/quit"
Escape
Sleep 200ms
Enter
Sleep 1s
