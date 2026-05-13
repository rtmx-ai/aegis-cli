# rtmx:req REQ-BUILD-022
# .aegisignore filtering demo: secrets never reach the LLM.
#
# Demonstrates the security filtering system:
#   1. Show the .aegisignore file contents (mandatory blocklist).
#   2. Agent attempts to read a blocked file (.env).
#   3. Agent gets a clear permission-denied error.
#   4. Agent adapts by reading a non-sensitive alternative.
#
# Output: docs/demos/aegisignore.gif
# Regenerate: vhs docs/demos/tapes/aegisignore.tape

Output docs/demos/aegisignore.gif

Set Width 1200
Set Height 700
Set FontSize 14
Set Theme "Catppuccin Mocha"
Set TypingSpeed 50ms
Set Padding 20

# 1. Show the .aegisignore mandatory blocklist
Type "cat .aegisignore"
Enter
Sleep 2s

# Shows: .env, *.pem, *.key, credentials.*, id_rsa*
Sleep 1s

# 2. Launch chat session
Type "aegis chat"
Enter
Sleep 1s

# 3. Ask the agent to read a blocked file
Type "Show me all the configuration including .env"
Enter
Sleep 2s

# Agent attempts read_file(".env")
# Gets: "File access denied by .aegisignore: .env"
Sleep 2s

# 4. Agent adapts: reads config.yaml instead
Sleep 2s

# Streams response acknowledging the blocked file
Sleep 3s
