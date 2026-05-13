# rtmx:req REQ-BUILD-020
# Audit ledger inspection demo: metadata-only immutable log.
#
# Demonstrates the audit ledger lifecycle:
#   1. Run a chat session that generates tool calls.
#   2. Inspect the JSONL ledger -- metadata only, never CUI.
#   3. Verify the hash chain integrity.
#   4. Search events by requirement ID.
#
# Output: docs/demos/audit-ledger.gif
# Regenerate: vhs docs/demos/tapes/audit-ledger.tape

Output docs/demos/audit-ledger.gif

Set Width 1200
Set Height 700
Set FontSize 14
Set Theme "Catppuccin Mocha"
Set TypingSpeed 50ms
Set Padding 20

# 1. Run a brief chat session that generates tool calls
Type "aegis chat"
Enter
Sleep 2s

Type "Read and fix the bug in parser.rs"
Enter
Sleep 3s

# Agent session generates tool calls -> audit events
Sleep 3s

# Exit chat
Ctrl+C
Sleep 1s

# 2. Inspect the audit ledger (JSONL, metadata only)
Type "cat ~/.aegis/logs/aegis-2026-03-25.jsonl | head -5 | jq ."
Enter
Sleep 3s

# Shows: SESSION_START, TOOL_PROPOSED (path only, no content),
# TOOL_APPROVED, TOOL_EXECUTED -- metadata only, never CUI
Sleep 3s

# 3. Verify hash chain integrity
Type "aegis audit verify"
Enter
Sleep 2s

# Hash chain intact: all entries link to previous hash
Sleep 2s

# 4. Search by requirement ID
Type "aegis audit search --req REQ-AGENT-001"
Enter
Sleep 2s

# Shows matching audit events for that requirement
Sleep 3s
