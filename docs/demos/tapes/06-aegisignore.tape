# .aegisignore: secrets never reach the LLM
# Shows: agent blocked from reading .env, adapts gracefully

Output docs/demos/gifs/06-aegisignore.gif
Set Width 120
Set Height 35
Set FontSize 14
Set Theme "Catppuccin Mocha"
Set TypingSpeed 50ms
Set Padding 20

Type "cat .aegisignore"
Enter
Sleep 2s

# Shows mandatory blocklist: .env, *.pem, *.key, credentials
Sleep 1s

Type "aegis chat"
Enter
Sleep 1s

Type "Show me all the configuration including .env"
Enter
Sleep 2s

# Agent attempts read_file(".env")
# Gets: "File access denied by .aegisignore: .env"
Sleep 2s

# Agent adapts: reads config.yaml instead
Sleep 2s

# Streams response acknowledging the blocked file
Sleep 3s
