@wip
Feature: In-TUI Authentication Display and User Feedback
  As a defense engineer using aegis in a terminal
  I need authentication flows to render inline without leaving the TUI
  And I need a privacy-respecting way to share feedback with the aegis team
  So that my workflow is uninterrupted and my voice is heard

  Background:
    Given aegis is configured with a valid config
    And the TUI is running

  # ---------------------------------------------------------------------------
  # REQ-TUI-065: Device code auth display
  # ---------------------------------------------------------------------------

  # @req REQ-TUI-065
  Scenario: Device code auth renders inline with clickable URL and user code
    Given the user runs "/connect vertex --project=my-proj"
    And the AuthManager initiates a GCP device code flow
    When a DeviceCodePending event is received by the TUI
    Then a system message should appear in the chat log containing:
      | Field           | Content                                      |
      | Provider        | "Google Cloud"                                |
      | Instruction     | "Visit the link below and enter the code"     |
      | URL             | An OSC 8 wrapped https:// URL                 |
      | Code            | A prominently displayed alphanumeric code      |

  # @req REQ-TUI-065
  Scenario: Auth spinner shows in status bar during device code polling
    Given a device code flow is in progress for "vertex"
    When the TUI renders the status bar
    Then the status bar should display "Authenticating to Google Cloud..." with an animated spinner
    And the phase indicator should show the polling state

  # @req REQ-TUI-065
  Scenario: Successful auth replaces spinner with connection confirmation
    Given a device code flow is in progress for "vertex"
    When a DeviceCodeComplete event is received
    Then the status bar spinner should be replaced with "Connected to Vertex AI"
    And a system message should appear: "Connected to Vertex AI (us-central1)"
    And the message should include the model name and token TTL

  # @req REQ-TUI-065
  Scenario: Auth timeout shows error with retry instructions
    Given a device code flow is in progress for "bedrock"
    When the 5-minute timeout elapses without approval
    Then a system message should appear containing "Authentication timed out"
    And the message should include "Try again with /connect bedrock"
    And the status bar should return to the idle state

  # @req REQ-TUI-065
  Scenario: User can cancel pending auth with Esc
    Given a device code flow is in progress for "vertex"
    When the user presses Esc
    Then the device code polling should be cancelled
    And a system message "Authentication cancelled" should appear
    And the TUI should return to normal input mode

  # @req REQ-TUI-065
  Scenario: User code is visually prominent and copyable
    Given a DeviceCodePending event with user_code "ABCD-EFGH"
    When the auth message is rendered
    Then the user code should be displayed with padding and emphasis
    And selecting the code with the mouse should copy the full code text
    And the code should be visually distinct from surrounding text

  # @req REQ-TUI-065
  Scenario: Auth display degrades gracefully in terminals without OSC 8
    Given the terminal does not support OSC 8 hyperlinks
    When a DeviceCodePending event is rendered
    Then the verification URL should appear as plain text
    And the user should be able to manually copy the URL

  # ---------------------------------------------------------------------------
  # REQ-TUI-066: /feedback slash command
  # ---------------------------------------------------------------------------

  # @req REQ-TUI-066
  Scenario: /feedback opens structured template for user input
    Given the user types "/feedback"
    When the command is executed
    Then the user's $EDITOR should open with a structured feedback template
    And the template should contain fields for satisfaction, what_worked, what_didnt, and feature_request
    And the satisfaction field should accept values 1-5

  # @req REQ-TUI-066
  Scenario: /feedback submits via GitHub issue when gh CLI is available
    Given the user completes the feedback template with satisfaction=4
    And the gh CLI is authenticated
    When the user saves and closes the editor
    Then a GitHub issue should be created on rtmx-ai/aegis-cli
    And the issue should have the label "user-feedback"
    And the issue title should include "User feedback: 4/5"
    And a system message should confirm "Feedback submitted -- thank you!"

  # @req REQ-TUI-066
  Scenario: /feedback falls back to clipboard URL when gh CLI unavailable
    Given the user completes the feedback template
    And the gh CLI is not installed or not authenticated
    When the user saves and closes the editor
    Then a pre-filled GitHub issue URL should be generated
    And the URL should be copied to the system clipboard
    And a system message should say "Feedback URL copied to clipboard. Open it in your browser to submit."

  # @req REQ-TUI-066
  Scenario: /feedback inline mode when $EDITOR is unset
    Given the EDITOR environment variable is not set
    When the user types "/feedback"
    Then the TUI should render an inline feedback form in the chat log
    And the user should be able to type responses directly
    And Enter on the last field should submit the feedback

  # @req REQ-TUI-066
  Scenario: Feedback prompt appears after configurable session count
    Given the user has completed 10 sessions (config feedback.prompt_after_sessions = 10)
    And the user has never submitted feedback or dismissed the prompt
    When a new session starts
    Then a one-time system message should appear:
      """
      You have used aegis for 10 sessions. Type /feedback to share your
      experience, or star us at github.com/rtmx-ai/aegis-cli
      """
    And the prompt should not appear again after being displayed

  # @req REQ-TUI-066
  Scenario: Feedback prompt is permanently dismissed after submission
    Given the user has submitted feedback via /feedback
    When subsequent sessions start
    Then the feedback prompt should never appear again
    And config.yaml should record feedback_submitted: true

  # @req REQ-TUI-066
  Scenario: No telemetry or PII is collected without user action
    Given the user has never typed /feedback
    When aegis runs for 100 sessions
    Then no data should be transmitted to any external service
    And no usage metrics should be collected or stored beyond session_count in config
    And no network requests should be made to rtmx.ai or github.com on behalf of feedback
