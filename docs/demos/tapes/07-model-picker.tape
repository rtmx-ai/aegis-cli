# Model picker: origin-aware model catalog with provenance display
#
# Shows the /model command palette with:
#   1. Model list with Origin and Status columns
#   2. Restricted model (PRC origin) shown grayed out with denial reason
#   3. Model switch to an approved model
#
# Covers: REQ-TUI-106 (origin column), REQ-TUI-107 (restricted display)

Output docs/demos/gifs/07-model-picker.gif

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

# 2. Open the model picker via slash command
Type "/model"
Sleep 1s

# 3. Command palette dropdown appears with model list:
#    Name          | Origin | Status     | Size
#    gemma4        | USA    | available  | 8B
#    llama3.3      | USA    | available  | 70B
#    granite-code  | USA    | available  | 34B
#    qwen2.5-coder | PRC    | restricted | 32B
#    deepseek-r1   | PRC    | restricted | 671B
Sleep 3s

# 4. Arrow down to a restricted model -- shown dim with denial reason
Down
Down
Down
Down
Sleep 2s

# 5. Attempt to select the restricted model -- inline error displayed:
#    "Restricted: PRC origin (denied by model origin policy)"
Enter
Sleep 3s

# 6. Select an approved model instead
Type "/model gemma4"
Enter
Sleep 2s

# 7. Model switches; status bar updates
Sleep 3s
