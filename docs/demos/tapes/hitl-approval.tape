# rtmx:req REQ-BUILD-018
# HITL approval flow demo: focused on the human-in-the-loop gate.
#
# Demonstrates that nothing mutates state without explicit approval:
#   1. Agent proposes a write; user presses N to deny.
#   2. Agent proposes an alternative write; user presses Y to approve.
#
# The gate dialog shows all four options: [Y] Approve [N] Deny [E] Edit [S] Skip.
# Fictional file paths so the tape is repo-state agnostic.
#
# Output: docs/demos/hitl-approval.gif
# Regenerate: vhs docs/demos/tapes/hitl-approval.tape
#   (or run: ./scripts/regen-demos.sh to regenerate all demo GIFs)

Output docs/demos/hitl-approval.gif

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

# 2. Ask for a change that the agent will implement in two steps
Type "Rotate the database credentials in config/prod.toml"
Enter
Sleep 1s

# 3. Agent reads the file and thinks about the proposed change
Sleep 3s

# 4. First proposal: overwrite the file in place.
#    Inline diff renders the old/new credential block.
Sleep 3s

# 5. HITL dialog: [Y] Approve [N] Deny [E] Edit [S] Skip
Sleep 2s

# 6. User denies -- prod creds should not be rewritten in place.
Type "n"
Enter
Sleep 2s

# 7. Agent acknowledges the denial and proposes a safer alternative:
#    write the new credential to config/prod.toml.new and leave a note.
Sleep 3s

# 8. Second HITL dialog for the safer proposal
Sleep 2s

# 9. User approves the alternative
Type "y"
Enter
Sleep 2s

# 10. Agent reports the safer write landed; audit event captured
Sleep 3s

# 11. Hold the final frame so the gate summary is readable
Sleep 2s
