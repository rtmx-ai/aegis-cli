Feature: Terminal UI Layout
  As a defense engineer using aegis in a terminal
  I need a clean, responsive single-pane interface
  So that I can focus on the AI interaction without visual clutter

  # @req REQ-TUI-001
  Scenario: Initial layout renders correctly
    Given a terminal of 80 columns by 24 rows
    When aegis starts in chat mode
    Then the top row should display the status line
    And the bottom rows should display the input prompt
    And the middle area should be the scrolling chat log

  # @req REQ-TUI-002
  Scenario: Streaming markdown renders incrementally
    Given an active aegis session
    When the LLM streams a markdown response with a code block
    Then the text should appear incrementally in the chat log
    And code blocks should be syntax-highlighted
    And the view should auto-scroll to follow new content

  # @req REQ-TUI-003
  Scenario: Diff rendering is compact and expandable
    Given the agent proposes a file change
    When the diff is rendered in the chat log
    Then it should be collapsed by default showing a hunk summary
    And pressing enter should expand to show the full diff
    And added lines should be green and removed lines should be red
