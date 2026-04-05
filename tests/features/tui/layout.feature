Feature: TUI Layout and Interactive Components
  As a defense engineer using aegis in a terminal
  I need a responsive, accessible TUI with streaming markdown, diffs, and tool calls
  So that I can interact with the agent efficiently across diverse terminal environments

  # ---------------------------------------------------------------------------
  # REQ-TUI-001: Single-pane chat layout with status line and input
  # ---------------------------------------------------------------------------

  # @req REQ-TUI-001
  Scenario: TUI renders three-region layout on startup
    Given aegis is configured with a valid config
    When the user launches "aegis chat"
    Then the TUI should display a status line occupying 1 row at the top
    And a scrolling chat log filling the middle region
    And a multi-line input area at the bottom
    And no persistent sidebar should be visible

  # @req REQ-TUI-001
  Scenario: TUI exits cleanly when terminal is too small
    Given the terminal is 20 columns by 5 rows
    When the user launches "aegis chat"
    Then aegis should display "Terminal too small (minimum 40x10)"
    And exit with a non-zero code

  # ---------------------------------------------------------------------------
  # REQ-TUI-002: Streaming markdown rendering with syntax highlighting
  # ---------------------------------------------------------------------------

  # @req REQ-TUI-002
  Scenario: Markdown streams at 30+ FPS without flicker
    Given the agent is streaming a response with markdown formatting
    When 500 characters of markdown arrive over 2 seconds
    Then the TUI should render at least 30 frames per second
    And no visual flicker should occur in the chat log region

  # @req REQ-TUI-002
  Scenario: Code blocks render with syntax highlighting
    Given the agent response contains a fenced code block with language "rust"
    When the response finishes streaming
    Then the code block should display with syntect-based highlighting
    And keywords like "fn" and "let" should be visually distinct from identifiers

  # @req REQ-TUI-002
  Scenario: Sticky scroll keeps latest content visible during streaming
    Given the chat log is scrolled to the bottom
    When new streaming content arrives
    Then the chat log should auto-scroll to keep the newest text visible
    And the scroll position should remain at the bottom

  # ---------------------------------------------------------------------------
  # REQ-TUI-003: Inline unified diff rendering for proposed file changes
  # ---------------------------------------------------------------------------

  # @req REQ-TUI-003
  Scenario: Diff renders collapsed by default showing hunk summary
    Given the agent proposes a file change to "src/main.rs"
    When the diff block renders in the chat log
    Then it should display a collapsed one-liner: "src/main.rs (+5, -2)"
    And the diff content should not be visible until expanded

  # @req REQ-TUI-003
  Scenario: Expanded diff shows red/green coloring for removed/added lines
    Given a collapsed diff block for "src/main.rs" is visible
    When the user presses Enter on the collapsed diff
    Then removed lines should render with a red background
    And added lines should render with a green background
    And context lines should render with the default background

  # @req REQ-TUI-003
  Scenario: Diff for a new file shows all lines as additions
    Given the agent proposes creating a new file "src/new_module.rs"
    When the diff block renders and is expanded
    Then all lines should display with a green background (additions)
    And the header should indicate "new file"

  # ---------------------------------------------------------------------------
  # REQ-TUI-004: Multi-line input with vim mode and history
  # ---------------------------------------------------------------------------

  # @req REQ-TUI-004
  Scenario: Vim mode keybindings work in the input area
    Given the TUI is active with vim mode enabled
    When the user presses "i" to enter insert mode and types "hello"
    And presses Escape to return to normal mode
    And presses "dd" to delete the line
    Then the input area should be empty

  # @req REQ-TUI-004
  Scenario: Up/Down arrow keys navigate command history
    Given the user has previously sent messages "first message" and "second message"
    When the user presses Up arrow twice in the input area
    Then the input area should display "first message"
    And pressing Down arrow should display "second message"

  # @req REQ-TUI-004
  Scenario: Shift+Enter inserts a newline without sending
    Given the TUI input area is focused
    When the user types "line one" and presses Shift+Enter
    And types "line two" and presses Enter
    Then the sent message should contain both "line one" and "line two" separated by a newline

  # ---------------------------------------------------------------------------
  # REQ-TUI-005: Inline tool call blocks with spinner and collapse
  # ---------------------------------------------------------------------------

  # @req REQ-TUI-005
  Scenario: Active tool call shows spinner during execution
    Given the agent has invoked "read_file" on "src/main.rs"
    And the tool call is still executing
    When the TUI renders the tool call block
    Then a spinner animation should be visible next to "read_file src/main.rs"

  # @req REQ-TUI-005
  Scenario: Completed tool call collapses to one-liner
    Given the "read_file" tool call for "src/main.rs" has completed
    When the TUI renders the tool call block
    Then it should display a collapsed one-liner: "read_file src/main.rs (done)"
    And the spinner should no longer be visible

  # @req REQ-TUI-005
  Scenario: Collapsed tool call is expandable for detail
    Given a completed and collapsed tool call block
    When the user presses Enter on the collapsed block
    Then the full tool output should be visible
    And pressing Enter again should collapse it back to one-liner

  # ---------------------------------------------------------------------------
  # REQ-TUI-006: Thinking animation with contextual verbs
  # ---------------------------------------------------------------------------

  # @req REQ-TUI-006
  Scenario: Thinking spinner shows rotating contextual verbs
    Given the agent is processing a prompt and has not yet produced output
    When the TUI renders the thinking indicator
    Then a spinner should display with verbs cycling through "Reading..." "Analyzing..." "Planning..."

  # @req REQ-TUI-006
  Scenario: Thinking spinner shows current tool name during execution
    Given the agent is executing the "grep" tool
    When the TUI renders the thinking indicator
    Then the spinner text should include "grep"

  # ---------------------------------------------------------------------------
  # REQ-TUI-007: Terminal resize handling with dynamic layout reflow
  # ---------------------------------------------------------------------------

  # @req REQ-TUI-007
  Scenario: TUI reflows layout on terminal resize
    Given the TUI is running in an 80x24 terminal
    When the terminal is resized to 120x40
    Then the layout should recompute all region sizes
    And the chat log should fill the expanded space
    And no rendering artifacts should appear

  # @req REQ-TUI-007
  Scenario: TUI handles rapid successive resizes without crashing
    Given the TUI is running
    When SIGWINCH is sent 10 times within 1 second
    Then the TUI should not crash
    And the final layout should match the final terminal dimensions

  # @req REQ-TUI-007
  Scenario: Scroll offset is clipped after shrinking terminal
    Given the chat log is scrolled to line 50 in a 100-line buffer
    When the terminal height shrinks from 40 rows to 10 rows
    Then the scroll offset should be adjusted so the visible content does not overflow

  # ---------------------------------------------------------------------------
  # REQ-TUI-008: Mouse scroll and click-to-select support
  # ---------------------------------------------------------------------------

  # @req REQ-TUI-008
  Scenario: Mouse wheel scrolls the chat log
    Given the chat log contains more content than fits on screen
    When the user scrolls the mouse wheel up by 3 ticks
    Then the chat log should scroll up by 9 lines (3 lines per tick)

  # @req REQ-TUI-008
  Scenario: Mouse click selects text in the chat log
    Given the chat log displays a response containing "important output"
    When the user clicks and drags to select "important output"
    Then the selected text should be visually highlighted

  # ---------------------------------------------------------------------------
  # REQ-TUI-009: Clipboard integration for copy/paste across OS and SSH
  # ---------------------------------------------------------------------------

  # @req REQ-TUI-009
  Scenario: Ctrl+C copies selected text to clipboard
    Given text "selected content" is highlighted in the chat log
    When the user presses Ctrl+C
    Then the system clipboard should contain "selected content"

  # @req REQ-TUI-009
  Scenario: OSC 52 clipboard passthrough works over SSH
    Given the user is connected via SSH with a terminal that supports OSC 52
    When the user copies text from the chat log
    Then the OSC 52 escape sequence should be emitted
    And the text should be available on the local clipboard

  # @req REQ-TUI-009
  Scenario: Ctrl+V pastes from clipboard into input area
    Given the system clipboard contains "paste this"
    When the user presses Ctrl+V in the input area
    Then the input area should contain "paste this"

  # ---------------------------------------------------------------------------
  # REQ-TUI-010: Slash commands: /clear /add /drop /context /help
  # ---------------------------------------------------------------------------

  # @req REQ-TUI-010
  Scenario: /clear resets the chat log without LLM round-trip
    Given the chat log contains previous conversation
    When the user types "/clear" and presses Enter
    Then the chat log should be empty
    And no request should be sent to the LLM provider

  # @req REQ-TUI-010
  Scenario: /help displays available commands
    Given the TUI is active
    When the user types "/help" and presses Enter
    Then the chat log should display a list including "/clear", "/add", "/drop", "/context", "/help"

  # @req REQ-TUI-010
  Scenario: Tab-completion completes slash commands
    Given the user has typed "/cl" in the input area
    When the user presses Tab
    Then the input area should auto-complete to "/clear"

  # @req REQ-TUI-010
  Scenario: Unknown slash command shows error
    Given the TUI is active
    When the user types "/nonexistent" and presses Enter
    Then the chat log should display "Unknown command: /nonexistent. Type /help for available commands."

  # ---------------------------------------------------------------------------
  # REQ-TUI-011: Session persistence and restore across restarts
  # ---------------------------------------------------------------------------

  # @req REQ-TUI-011
  Scenario: Conversation is persisted to session file on exit
    Given the user has an active conversation with 5 messages
    When the user exits aegis
    Then a file matching "~/.aegis/sessions/<uuid>.jsonl" should exist
    And it should contain 5 message entries in JSONL format

  # @req REQ-TUI-011
  Scenario: Session restore prompt on restart
    Given "~/.aegis/sessions/" contains a previous session file
    When the user launches "aegis chat"
    Then aegis should prompt "[R]esume / [N]ew"
    And selecting "R" should restore the previous conversation in the chat log

  # @req REQ-TUI-011
  Scenario: New session is started when no previous session exists
    Given "~/.aegis/sessions/" is empty
    When the user launches "aegis chat"
    Then no resume prompt should appear
    And the chat log should be empty

  # ---------------------------------------------------------------------------
  # REQ-TUI-012: Color theme support with light/dark and 256-color fallback
  # ---------------------------------------------------------------------------

  # @req REQ-TUI-012
  Scenario: Dark theme renders correctly on dark terminal background
    Given the terminal has a dark background and COLORTERM is set to "truecolor"
    When the user launches "aegis chat"
    Then the TUI should render with the dark color theme
    And text should be legible with sufficient contrast

  # @req REQ-TUI-012
  Scenario: Light theme activates with --theme light flag
    Given the user launches "aegis chat --theme light"
    When the TUI renders
    Then the color scheme should use dark text on a light background

  # @req REQ-TUI-012
  Scenario: 256-color fallback when COLORTERM is not truecolor
    Given COLORTERM is unset and TERM is "xterm-256color"
    When the user launches "aegis chat"
    Then the TUI should use 256-color palette instead of truecolor

  # ---------------------------------------------------------------------------
  # REQ-TUI-013: --no-tui plain-text mode for screen readers
  # ---------------------------------------------------------------------------

  # @req REQ-TUI-013
  Scenario: --no-tui outputs plain text without escape codes
    Given the user launches "aegis chat --no-tui"
    When the agent responds with "hello world"
    Then stdout should contain "hello world" with no ANSI escape sequences

  # @req REQ-TUI-013
  Scenario: NO_COLOR environment variable activates plain-text mode
    Given NO_COLOR is set in the environment
    When the user launches "aegis chat"
    Then stdout output should contain no ANSI escape sequences

  # @req REQ-TUI-013
  Scenario: TERM=dumb activates plain-text mode
    Given TERM is set to "dumb"
    When the user launches "aegis chat"
    Then the TUI should not render
    And plain-text mode should be used instead

  # ---------------------------------------------------------------------------
  # REQ-TUI-014: SSH and tmux rendering compatibility
  # ---------------------------------------------------------------------------

  # @req REQ-TUI-014
  Scenario: TUI renders without artifacts under tmux
    Given the user is running under tmux with TERM set to "tmux-256color"
    When "aegis chat" is launched
    Then the TUI should render without visual artifacts
    And smcup/rmcup alternate screen sequences should be used

  # @req REQ-TUI-014
  Scenario: Mouse capture is disabled under TERM=screen
    Given the user is running under GNU screen with TERM set to "screen"
    When "aegis chat" is launched
    Then mouse capture should be disabled
    And keyboard navigation should remain fully functional

  # @req REQ-TUI-014
  Scenario: TUI renders correctly over SSH connection
    Given the user is connected via SSH with TERM set to "xterm-256color"
    When "aegis chat" is launched
    Then the TUI should render correctly
    And truecolor detection should fall back based on SSH terminal capabilities

  # ---------------------------------------------------------------------------
  # REQ-TUI-015: Inline dismissible error banners
  # ---------------------------------------------------------------------------

  # @req REQ-TUI-015
  Scenario: Transient error displays as red banner above input
    Given the LLM provider returns a transient 503 error
    When the error is surfaced to the TUI
    Then a red banner should appear above the input area
    And the banner should contain the error message
    And the chat log content should not be overwritten

  # @req REQ-TUI-015
  Scenario: Error banner auto-dismisses after 8 seconds
    Given an error banner is currently displayed
    When 8 seconds elapse
    Then the error banner should disappear automatically

  # @req REQ-TUI-015
  Scenario: Fatal error banner offers Quit or Retry
    Given the LLM provider returns a fatal authentication error
    When the error is surfaced to the TUI
    Then the banner should display "[Q]uit / [R]etry" options
    And pressing "Q" should exit aegis
    And pressing "R" should retry the last operation

  # ---------------------------------------------------------------------------
  # REQ-TUI-016: Progress indicators for long tool calls
  # ---------------------------------------------------------------------------

  # @req REQ-TUI-016
  Scenario: Elapsed time counter appears after 3 seconds
    Given a tool call has been executing for 4 seconds
    When the TUI renders the tool call block
    Then an elapsed time counter in MM:SS format should be visible
    And it should show "00:04"

  # @req REQ-TUI-016
  Scenario: Byte-progress bar renders for file operations
    Given the "read_file" tool is reading a 1 MB file
    When the TUI renders the progress indicator
    Then a progress bar should display bytes read vs total bytes

  # @req REQ-TUI-016
  Scenario: No progress indicator for tool calls completing under 3 seconds
    Given a tool call completes in 1 second
    When the TUI renders the tool call block
    Then no elapsed time counter should have been displayed

  # ---------------------------------------------------------------------------
  # REQ-TUI-017: Conversation history navigation with search
  # ---------------------------------------------------------------------------

  # @req REQ-TUI-017
  Scenario: PgUp/PgDn scrolls through conversation history
    Given the chat log contains 200 lines of conversation
    When the user presses PgUp
    Then the chat log should scroll up by one page
    And pressing PgDn should scroll back down by one page

  # @req REQ-TUI-017
  Scenario: Forward-slash opens incremental search in chat log
    Given the chat log contains text "error: connection refused"
    When the user presses "/" and types "connection"
    Then the first match "connection" should be highlighted in the chat log
    And pressing "n" should jump to the next match
    And pressing "N" should jump to the previous match

  # @req REQ-TUI-017
  Scenario: Esc exits search mode
    Given the user is in search mode with matches highlighted
    When the user presses Esc
    Then search mode should exit
    And the highlights should be removed

  # ---------------------------------------------------------------------------
  # REQ-TUI-018: Interactive file picker for @-mention context injection
  # ---------------------------------------------------------------------------

  # @req REQ-TUI-018
  Scenario: Typing @ opens the fuzzy file picker overlay
    Given the TUI input area is focused in a project directory with files
    When the user types "@"
    Then a floating overlay should appear listing project files

  # @req REQ-TUI-018
  Scenario: Fuzzy filter narrows file picker results
    Given the file picker overlay is open
    When the user types "main"
    Then the file list should filter to show only files matching "main"
    And pressing Enter should insert the selected file path into the input

  # @req REQ-TUI-018
  Scenario: Esc closes the file picker without selection
    Given the file picker overlay is open
    When the user presses Esc
    Then the overlay should close
    And no file path should be inserted into the input

  # ---------------------------------------------------------------------------
  # REQ-TUI-019: Token usage display in status line
  # ---------------------------------------------------------------------------

  # @req REQ-TUI-019
  Scenario: Status line shows running token counts
    Given the agent has consumed 1500 input tokens and 800 output tokens
    When the TUI renders the status line
    Then it should display "Tokens: 1500 in / 800 out"

  # @req REQ-TUI-019
  Scenario: Token counts accumulate across multiple exchanges
    Given the session has had 3 exchanges totaling 5000 input and 3000 output tokens
    When the TUI renders the status line
    Then it should display the cumulative "Tokens: 5000 in / 3000 out"

  # ---------------------------------------------------------------------------
  # REQ-TUI-020: Cost tracking display with session totals
  # ---------------------------------------------------------------------------

  # @req REQ-TUI-020
  Scenario: Status line shows estimated USD cost
    Given the session has consumed tokens with a model costing $3.00/MTok input
    And the total input tokens are 10000 and output tokens are 5000
    When the TUI renders the status line
    Then a cost estimate should be visible in the status line

  # @req REQ-TUI-020
  Scenario: /cost command shows detailed cost breakdown
    Given the session has had multiple exchanges across different models
    When the user types "/cost" and presses Enter
    Then a table should render showing per-model token counts and costs
    And a total session cost at the bottom

  # @req REQ-TUI-020
  Scenario: Local models show $0.00 cost
    Given the session is using a local Ollama model
    When the TUI renders the status line
    Then the cost display should show "$0.00"

  # ---------------------------------------------------------------------------
  # REQ-TUI-024: /add and /drop commands for context file management
  # ---------------------------------------------------------------------------

  # @req REQ-TUI-024
  Scenario: /add loads a single file into the agent working context
    Given the TUI is active in a project directory containing "src/main.rs"
    When the user types "/add src/main.rs" and presses Enter
    Then the chat log should display "Added src/main.rs to context"
    And the file "src/main.rs" should be included in the agent working context

  # @req REQ-TUI-024
  Scenario: /add supports glob patterns to load multiple files
    Given the TUI is active in a project directory containing "src/lib.rs", "src/main.rs", and "src/utils.rs"
    When the user types "/add src/*.rs" and presses Enter
    Then the chat log should display "Added 3 files to context"
    And all three files should be included in the agent working context

  # @req REQ-TUI-024
  Scenario: /add with a nonexistent path shows an error
    Given the TUI is active
    When the user types "/add no_such_file.rs" and presses Enter
    Then the chat log should display "No files matched: no_such_file.rs"
    And the agent working context should remain unchanged

  # @req REQ-TUI-024
  Scenario: /drop removes a file from the agent working context
    Given the file "src/main.rs" is loaded in the agent working context
    When the user types "/drop src/main.rs" and presses Enter
    Then the chat log should display "Dropped src/main.rs from context"
    And the file "src/main.rs" should no longer be in the agent working context

  # @req REQ-TUI-024
  Scenario: /drop without arguments lists currently loaded files
    Given files "src/main.rs" and "src/lib.rs" are loaded in the agent working context
    When the user types "/drop" and presses Enter
    Then the chat log should display "Context files:" followed by "src/main.rs" and "src/lib.rs"
    And no files should be removed from the context

  # @req REQ-TUI-024
  Scenario: /context reflects files added via /add
    Given the user has run "/add src/main.rs" and "/add src/lib.rs"
    When the user types "/context" and presses Enter
    Then the chat log should list "src/main.rs" and "src/lib.rs" under loaded context files

  # @req REQ-TUI-024
  Scenario: /drop a file that is not in context shows an error
    Given the agent working context is empty
    When the user types "/drop src/main.rs" and presses Enter
    Then the chat log should display "File not in context: src/main.rs"

  # ---------------------------------------------------------------------------
  # REQ-TUI-025: /model command switches LLM model for current session
  # ---------------------------------------------------------------------------

  # @req REQ-TUI-025
  Scenario: /model with no arguments shows the current model
    Given the session is using model "claude-sonnet-4-20250514"
    When the user types "/model" and presses Enter
    Then the chat log should display "Current model: claude-sonnet-4-20250514"

  # @req REQ-TUI-025
  Scenario: /model switches to a valid model
    Given the session is using model "claude-sonnet-4-20250514"
    And the LLM provider supports model "mixtral-8x7b"
    When the user types "/model mixtral-8x7b" and presses Enter
    Then the chat log should display "Switched model to mixtral-8x7b"
    And subsequent agent requests should use model "mixtral-8x7b"

  # @req REQ-TUI-025
  Scenario: /model with an invalid model name shows an error
    Given the LLM provider does not support model "nonexistent-model"
    When the user types "/model nonexistent-model" and presses Enter
    Then the chat log should display "Unknown model: nonexistent-model. Use /model to see available models."

  # @req REQ-TUI-025
  Scenario: Model switch is recorded in the audit ledger
    Given the session is using model "claude-sonnet-4-20250514"
    And the LLM provider supports model "mixtral-8x7b"
    When the user types "/model mixtral-8x7b" and presses Enter
    Then the audit ledger should contain an entry with event type "model_switch"
    And the entry should record the previous model "claude-sonnet-4-20250514" and the new model "mixtral-8x7b"

  # @req REQ-TUI-025
  Scenario: /model lists available models when provider supports enumeration
    Given the LLM provider supports models "claude-sonnet-4-20250514", "mixtral-8x7b", and "llama3-70b"
    When the user types "/model --list" and presses Enter
    Then the chat log should display all three available model names

  # ---------------------------------------------------------------------------
  # REQ-TUI-026: /infra command for plugin operations from TUI
  # ---------------------------------------------------------------------------

  # @req REQ-TUI-026
  Scenario: /infra with no arguments lists discovered plugins
    Given two plugins "gcp-assured-workloads" and "aws-govcloud" are discovered
    When the user types "/infra" and presses Enter
    Then the chat log should display "Discovered plugins:" followed by "gcp-assured-workloads" and "aws-govcloud"

  # @req REQ-TUI-026
  Scenario: /infra status runs plugin health check and streams events
    Given the plugin "gcp-assured-workloads" is discovered
    When the user types "/infra status gcp-assured-workloads" and presses Enter
    Then the plugin "gcp-assured-workloads" should receive the "status" subcommand
    And NDJSON progress events should render as system messages in the chat log
    And the final result event should display the plugin health summary

  # @req REQ-TUI-026
  Scenario: /infra preview shows a dry-run of changes
    Given the plugin "gcp-assured-workloads" is discovered
    When the user types "/infra preview gcp-assured-workloads" and presses Enter
    Then the plugin should receive the "preview" subcommand
    And diagnostic events should render in the chat log showing planned resource changes
    And no resources should be created or modified

  # @req REQ-TUI-026
  Scenario: /infra up requires HITL approval before provisioning
    Given the plugin "gcp-assured-workloads" is discovered
    When the user types "/infra up gcp-assured-workloads" and presses Enter
    Then a HITL approval prompt should appear with risk level "high"
    And the operation should block until the user approves or denies

  # @req REQ-TUI-026
  Scenario: /infra up proceeds after HITL approval
    Given a HITL approval prompt is displayed for "/infra up gcp-assured-workloads"
    When the user approves the operation
    Then the plugin should receive the "up" subcommand
    And NDJSON progress events should stream to the chat log in real time
    And the final result event should display the provisioning summary

  # @req REQ-TUI-026
  Scenario: /infra destroy requires HITL approval and typed confirmation
    Given the plugin "gcp-assured-workloads" is discovered
    When the user types "/infra destroy gcp-assured-workloads" and presses Enter
    Then a HITL approval prompt should appear with risk level "critical"
    And the user must type the plugin name "gcp-assured-workloads" to confirm
    And the operation should block until confirmation is provided

  # @req REQ-TUI-026
  Scenario: /infra destroy is aborted when confirmation text does not match
    Given a HITL approval prompt is displayed for "/infra destroy gcp-assured-workloads"
    When the user types "wrong-name" as confirmation
    Then the chat log should display "Confirmation mismatch. Destroy aborted."
    And no destroy subcommand should be sent to the plugin

  # @req REQ-TUI-026
  Scenario: Plugin NDJSON events display correctly in chat log
    Given the plugin "gcp-assured-workloads" is executing the "up" subcommand
    When the plugin emits progress, diagnostic, check, and result events
    Then progress events should render with a spinner and percentage
    And diagnostic events should render as indented system messages
    And check events should render with pass/fail indicators
    And the result event should render as a summary block

  # ---------------------------------------------------------------------------
  # REQ-TUI-027: /undo command reverts last approved write operation
  # ---------------------------------------------------------------------------

  # @req REQ-TUI-027
  Scenario: /undo reverts the last approved write_file operation
    Given the agent wrote "new content" to "src/main.rs" replacing "old content"
    And the write was approved via HITL gate
    When the user types "/undo" and presses Enter
    And the user confirms the undo operation
    Then the file "src/main.rs" should contain "old content"
    And the chat log should display "Reverted src/main.rs to previous state"

  # @req REQ-TUI-027
  Scenario: /undo --all reverts all session write operations
    Given the agent wrote to "src/main.rs" and "src/lib.rs" during the session
    When the user types "/undo --all" and presses Enter
    And the user confirms the undo operation
    Then both "src/main.rs" and "src/lib.rs" should be restored to their pre-write content
    And the chat log should display "Reverted 2 files to previous state"

  # @req REQ-TUI-027
  Scenario: /undo when no writes have occurred shows informational message
    Given no write_file operations have been approved in the session
    When the user types "/undo" and presses Enter
    Then the chat log should display "Nothing to undo"

  # @req REQ-TUI-027
  Scenario: /undo requires confirmation before executing
    Given the agent wrote to "src/main.rs" during the session
    When the user types "/undo" and presses Enter
    Then a confirmation prompt should appear: "Revert src/main.rs? [y/N]"
    And the revert should not execute until the user confirms with "y"

  # @req REQ-TUI-027
  Scenario: /undo is cancelled when user denies confirmation
    Given the agent wrote to "src/main.rs" during the session
    When the user types "/undo" and presses Enter
    And the user responds with "N" to the confirmation prompt
    Then the chat log should display "Undo cancelled"
    And the file "src/main.rs" should remain unchanged

  # @req REQ-TUI-027
  Scenario: /undo is logged to the audit ledger
    Given the agent wrote to "src/main.rs" during the session
    When the user types "/undo" and confirms the operation
    Then the audit ledger should contain an entry with event type "undo"
    And the entry should record the file path "src/main.rs" and the session ID

  # ---------------------------------------------------------------------------
  # REQ-TUI-028: /doctor command runs connectivity and health checks
  # ---------------------------------------------------------------------------

  # @req REQ-TUI-028
  Scenario: /doctor checks LLM endpoint reachability and shows latency
    Given the LLM provider is configured with endpoint "https://vertex.googleapis.com"
    When the user types "/doctor" and presses Enter
    Then the chat log should display a system message "LLM endpoint: OK (latency: <N>ms)"
    And the latency value should reflect the actual round-trip time

  # @req REQ-TUI-028
  Scenario: /doctor reports unreachable LLM endpoint
    Given the LLM provider endpoint is unreachable
    When the user types "/doctor" and presses Enter
    Then the chat log should display a system message "LLM endpoint: UNREACHABLE"
    And the message should include the endpoint URL and error reason

  # @req REQ-TUI-028
  Scenario: /doctor checks plugin discovery and status
    Given plugins "gcp-assured-workloads" and "aws-govcloud" are on the PATH
    When the user types "/doctor" and presses Enter
    Then the chat log should display "Plugins: 2 discovered"
    And each plugin should show its name and version

  # @req REQ-TUI-028
  Scenario: /doctor reports when no plugins are discovered
    Given no aegis-infra plugins are on the PATH
    When the user types "/doctor" and presses Enter
    Then the chat log should display "Plugins: none discovered"

  # @req REQ-TUI-028
  Scenario: /doctor checks configuration validity
    Given a valid aegis configuration file exists at "~/.aegis/config.toml"
    When the user types "/doctor" and presses Enter
    Then the chat log should display "Config: OK"

  # @req REQ-TUI-028
  Scenario: /doctor reports configuration errors
    Given the aegis configuration file at "~/.aegis/config.toml" has an invalid provider entry
    When the user types "/doctor" and presses Enter
    Then the chat log should display "Config: ERROR" followed by the validation error details

  # @req REQ-TUI-028
  Scenario: /doctor displays all results as system messages
    Given the LLM endpoint is reachable and 1 plugin is discovered and config is valid
    When the user types "/doctor" and presses Enter
    Then three system messages should appear in the chat log
    And each message should be prefixed with a check category label
    And the results should appear in order: LLM endpoint, plugins, config
