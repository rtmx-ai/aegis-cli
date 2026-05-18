# Model download: pull models from Ollama with policy gate
#
# Shows the /model download flow:
#   1. Attempt to download a restricted model -- blocked by policy
#   2. Download an approved model with progress bar
#   3. Air-gapped mode disables download entirely
#
# Covers: REQ-TUI-108 (download command), REQ-TUI-109 (progress bar),
#         REQ-TUI-110 (air-gapped manifest)

Output docs/demos/gifs/08-model-download.gif

Set Width 1200
Set Height 700
Set FontSize 14
Set Theme "Catppuccin Mocha"
Set TypingSpeed 50ms
Set Padding 20

# 1. Launch the chat TUI
Type "aegis chat"
Enter
Sleep 2s

# 2. Attempt to download a restricted model
Type "/model download deepseek-r1"
Enter
Sleep 2s

# 3. Policy gate blocks the download:
#    "Download denied: PRC origin (denied by model origin policy)"
Sleep 3s

# 4. Download an approved model
Type "/model download gemma4"
Enter
Sleep 1s

# 5. Progress bar appears in chat area:
#    "downloading gemma4: 1.2/4.8 GB (25%) ~3m15s"
Sleep 3s

# 6. Progress updates
Sleep 3s

# 7. Download completes; model appears in picker as available
Sleep 3s
