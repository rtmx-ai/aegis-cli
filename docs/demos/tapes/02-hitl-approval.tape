# HITL approval flow: every mutation requires human consent
# Shows: diff rendering, Y/N/E/S dialog, audit logging

Output docs/demos/gifs/02-hitl-approval.gif
Set Width 120
Set Height 35
Set FontSize 14
Set Theme "Catppuccin Mocha"
Set TypingSpeed 50ms
Set Padding 20

Type "aegis chat"
Enter
Sleep 1s

Type "Add error handling to the auth token refresh"
Enter
Sleep 2s

# Agent reads files
Sleep 2s

# Agent proposes write_file with inline diff
# Red/green unified diff appears
Sleep 3s

# HITL dialog: [Y] Approve [N] Deny [E] Edit [S] Skip
Sleep 2s

# User approves
Type "y"
Enter
Sleep 1s

# Agent proposes a second change
Sleep 2s

# User edits the proposed command
Type "e"
Sleep 1s

# Modified command shown
Sleep 2s
Enter
Sleep 2s
