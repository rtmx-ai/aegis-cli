# rtmx:req REQ-BUILD-019
# Air-gapped local model demo: zero network egress
#
# Demonstrates aegis running entirely offline with a local Ollama model.
# Key message: no network calls leave the machine -- suitable for SIPR
# and other disconnected environments.

Output docs/demos/airgapped.gif
Set Width 1200
Set Height 700
Set FontSize 14
Set Theme "Catppuccin Mocha"
Set TypingSpeed 50ms
Set Padding 20

# Initialize aegis in local/air-gapped mode
Type "aegis init --local"
Enter
Sleep 3s

# Config written: provider=ollama, model=llama3, endpoint=localhost:11434
# No external network calls -- all traffic stays on loopback.
Sleep 1s

# Launch the TUI chat session
Type "aegis chat"
Enter
Sleep 2s

# Splash screen appears, then auto-dismisses
Sleep 2s

# User asks a realistic coding question
Type "Read the main.rs file and suggest how to add structured logging with tracing"
Enter
Sleep 2s

# Agent uses local tools: list_dir, read_file -- all filesystem, no network
Sleep 4s

# Ollama streams the response over localhost:11434
# Status bar shows: (>) aegis | llama3 | Local (no egress)
Sleep 5s

# Response includes a code block with tracing setup
Sleep 3s

# Demonstrate the key point: zero network egress
Type "/doctor"
Enter
Sleep 2s

# Doctor output confirms:
#   [PASS] LLM endpoint: ollama (localhost:11434)
#   [PASS] Network mode: air-gapped (no egress)
#   [PASS] All tools local
Sleep 3s

# Clean exit
Type "/quit"
Enter
Sleep 1s
