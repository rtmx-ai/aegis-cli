# Air-gapped mode: works offline with local models
# Shows: local init, Ollama provider, zero network egress

Output docs/demos/gifs/03-airgapped.gif
Set Width 120
Set Height 35
Set FontSize 14
Set Theme "Catppuccin Mocha"
Set TypingSpeed 50ms
Set Padding 20

Type "aegis init --local"
Enter
Sleep 2s

# Config written to ~/.aegis/config.yaml
# Status line: Local (Ollama llama3)
Sleep 1s

Type "aegis chat"
Enter
Sleep 1s

Type "List the files in this project and summarize the architecture"
Enter
Sleep 2s

# Agent uses list_dir, read_file -- all local
Sleep 4s

# Streaming response from local Ollama
Sleep 4s

# No network indicator in status line
Sleep 2s
