# Hero demo: 30-second pitch for README top
# Shows: aegis init --local, chat session, streaming response, tool calls

Output docs/demos/gifs/01-hero.gif
Set Width 120
Set Height 35
Set FontSize 14
Set Theme "Catppuccin Mocha"
Set TypingSpeed 50ms
Set Padding 20

Type "aegis init --local"
Enter
Sleep 2s

Type "aegis chat"
Enter
Sleep 1s

Type "Explain the auth module and fix the failing test"
Enter
Sleep 500ms

# Agent reads files
Sleep 3s

# Agent streams markdown response with syntax highlighting
Sleep 4s

# Agent proposes a fix (inline diff)
Sleep 3s

# User approves
Type "y"
Enter
Sleep 2s

# Agent runs tests
Sleep 3s

# Session complete
Sleep 2s
