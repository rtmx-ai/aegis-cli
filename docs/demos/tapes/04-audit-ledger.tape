# Audit ledger: every action logged, metadata only
# Shows: session with tool calls, then ledger inspection

Output docs/demos/gifs/04-audit-ledger.gif
Set Width 120
Set Height 35
Set FontSize 14
Set Theme "Catppuccin Mocha"
Set TypingSpeed 50ms
Set Padding 20

Type "aegis chat"
Enter
Sleep 1s

Type "Read and fix the bug in parser.rs"
Enter
Sleep 3s

# Agent session completes
Sleep 3s

# Exit chat
Ctrl+C
Sleep 1s

# Inspect the audit ledger
Type "cat ~/.aegis/logs/aegis-2026-03-25.jsonl | head -5 | jq ."
Enter
Sleep 3s

# Shows: SESSION_START, TOOL_PROPOSED (path only, no content),
# TOOL_APPROVED, TOOL_EXECUTED -- metadata only, never CUI
Sleep 3s
