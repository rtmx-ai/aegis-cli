# HITL approval flow: every mutation requires human consent
#
# Expected outcome:
#   1. Launch aegis chat
#   2. Ask the agent to read a file (triggers read_file tool)
#   3. HITL approval dialog appears
#   4. User approves with "a"
#   5. Agent returns file content summary
#   6. /quit exits cleanly

Output docs/demos/gifs/02-hitl-approval.gif
Set Width 1200
Set Height 700
Set FontSize 14
Set Theme "Catppuccin Mocha"
Set TypingSpeed 40ms
Set Padding 20

# Clear stale session
Hide
Type "rm -f ~/.aegis/sessions/current.json"
Enter
Sleep 500ms
Show

Type "aegis chat"
Enter
Sleep 5s

# Ask a question that triggers read_file tool
Type "Read the CLAUDE.md file and summarize it"
Enter

# Wait for agent response and possible tool call
Sleep 15s

# If tool approval appears, approve it
Type "a"
Sleep 10s

# Show the streamed result
Sleep 5s

# Exit
Type@0ms "/quit"
Escape
Sleep 200ms
Enter
Sleep 1s
