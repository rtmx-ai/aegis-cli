# Air-gapped mode: works offline with local models
#
# Expected outcome:
#   1. Launch aegis chat (connected to local Ollama)
#   2. /doctor shows local connectivity health check
#   3. /model shows the active local model
#   4. Ask a question, see local model response
#   5. /trivia shows a defense AI fact
#   6. /quit exits cleanly

Output docs/demos/gifs/03-airgapped.gif
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

# Show doctor to verify local connectivity
Type@0ms "/doctor"
Escape
Sleep 200ms
Enter
Sleep 4s

# Show the model
Type@0ms "/model"
Escape
Sleep 200ms
Enter
Sleep 3s

# Ask a simple question
Type "What is aegis?"
Enter
Sleep 15s

# Show trivia
Type@0ms "/trivia"
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
