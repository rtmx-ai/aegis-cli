# rtmx:req REQ-BUILD-017
# Hero demo: the 30-45 second pitch shown at the top of README.md.
#
# Shows the full aegis REA loop in a single clip:
#   1. Launch aegis chat in a project directory.
#   2. Ask a coding question (streamed response with syntax highlighting).
#   3. Agent proposes a file edit via the HITL gate.
#   4. User approves the change; the edit is applied.
#
# File paths referenced in the prompt are fictional so that the tape can
# be regenerated against any repo state (e.g. in CI).
#
# Output: docs/demos/hero.gif
# Regenerate: vhs docs/demos/tapes/hero.tape
#   (or run: ./scripts/regen-demos.sh to regenerate all demo GIFs)

Output docs/demos/hero.gif

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

# 2. Ask a coding question
Type "Add retry logic with exponential backoff to src/net/client.rs"
Enter
Sleep 1s

# 3. Agent reads context (status bar: "reading 2 files")
Sleep 3s

# 4. Agent streams a markdown explanation with syntax-highlighted Rust
Sleep 5s

# 5. Agent proposes a write_file tool call; inline diff renders
#    (red/green unified diff showing the retry wrapper).
Sleep 4s

# 6. HITL approval dialog: [Y] Approve  [N] Deny  [E] Edit  [S] Skip
Sleep 2s

# 7. User approves the proposed edit
Type "y"
Enter
Sleep 2s

# 8. Agent confirms the file was written and suggests a follow-up test
Sleep 3s

# 9. Hold the final frame so viewers can read the summary
Sleep 3s
