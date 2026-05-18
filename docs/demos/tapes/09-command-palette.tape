# Command palette: slash commands with autocomplete
#
# Shows the slash command system:
#   1. Type / to see available commands
#   2. Tab-complete a command
#   3. /context to see active file context
#   4. /clear to reset the session
#
# Covers: REQ-TUI-010 (slash commands), REQ-TUI-024..028 (palette)

Output docs/demos/gifs/09-command-palette.gif

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

# 2. Type / to open the command palette
Type "/"
Sleep 2s

# 3. Dropdown shows available commands:
#    /add      Add files to context
#    /clear    Clear conversation
#    /context  Show active context
#    /drop     Remove files from context
#    /help     Show help
#    /model    Switch model
Sleep 3s

# 4. Start typing to filter
Type "con"
Sleep 1s

# 5. Tab-complete to /context
Tab
Sleep 1s
Enter
Sleep 2s

# 6. Context panel shows active files and token counts
Sleep 3s

# 7. Add a file to context
Type "/add src/lib.rs"
Enter
Sleep 2s

# 8. Clear the session
Type "/clear"
Enter
Sleep 2s

# 9. Clean slate
Sleep 2s
