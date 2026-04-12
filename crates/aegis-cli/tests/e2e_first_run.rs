//! End-to-end first-run user journey test.
//!
//! REQ-TEST-025: Simulates the complete first-run experience from a
//! fresh install through tutorial completion, exercising the real
//! onboarding modules with an isolated HOME directory.

use aegis_onboard::init::{InitInputs, run_init};
use aegis_onboard::tutorial::{
    TutorialState, is_first_run, is_tutorial_complete, mark_tutorial_complete,
    run_tutorial_steps, welcome_message,
};
use aegis_test_support::isolation::IsolatedHome;

// rtmx:req REQ-TEST-025
#[test]
fn test_first_run_user_journey() {
    let home = IsolatedHome::new().expect("create isolated home");
    let config_dir = home.aegis_dir();
    let config_path = home.config_path();

    // -- Phase 1: Fresh install detection --
    // No config.yaml exists yet, so this is a first run.
    assert!(
        is_first_run(&config_dir),
        "Must detect first run when no config.yaml exists"
    );
    assert!(
        !is_tutorial_complete(&config_dir),
        "Tutorial must not be marked complete on fresh install"
    );

    // -- Phase 2: Welcome message is available --
    let welcome = welcome_message();
    assert!(!welcome.is_empty(), "Welcome message must not be empty");
    assert!(
        welcome.contains("aegis"),
        "Welcome message must mention aegis"
    );

    // -- Phase 3: Tutorial steps are defined and walkable --
    let steps = run_tutorial_steps();
    assert_eq!(steps.len(), 3, "Tutorial must have exactly 3 steps");
    assert_eq!(steps[0].id, "mode_selection");
    assert_eq!(steps[1].id, "backend_config");
    assert_eq!(steps[2].id, "connectivity_test");

    // Walk through the tutorial state machine.
    let mut state = TutorialState::new();
    assert_eq!(
        state.current().unwrap().id,
        "mode_selection",
        "First step must be mode selection"
    );
    assert!(state.advance(), "Should advance to backend_config");
    assert_eq!(state.current().unwrap().id, "backend_config");
    assert!(state.advance(), "Should advance to connectivity_test");
    assert_eq!(state.current().unwrap().id, "connectivity_test");
    assert!(!state.advance(), "Should not advance past the last step");

    // -- Phase 4: Backend selection via init state machine --
    // Simulate choosing local mode (air-gapped, no network needed).
    let inputs = InitInputs::local();
    let result = run_init(&inputs, &config_path).expect("local init must succeed");
    assert!(
        result.config_path.exists(),
        "Config file must be written after init"
    );
    assert_eq!(
        result.mode,
        aegis_onboard::config::Mode::Local,
        "Mode must be Local after local init"
    );

    // -- Phase 5: Config is valid and loadable --
    let config =
        aegis_onboard::config::AegisConfig::load(&config_path).expect("config must load");
    assert_eq!(config.mode, aegis_onboard::config::Mode::Local);
    assert_eq!(config.backend.provider, "local");

    // -- Phase 6: No longer a first run --
    assert!(
        !is_first_run(&config_dir),
        "Must NOT be first run after config.yaml is written"
    );

    // -- Phase 7: Mark tutorial complete --
    mark_tutorial_complete(&config_dir).expect("marking tutorial complete must succeed");
    assert!(
        is_tutorial_complete(&config_dir),
        "Tutorial must be marked complete after mark_tutorial_complete()"
    );

    // Idempotent: marking again should not fail.
    mark_tutorial_complete(&config_dir).expect("second mark must also succeed");
    assert!(is_tutorial_complete(&config_dir));

    // -- Phase 8: Re-init does not break completed state --
    // Re-running init should succeed (merge config) and tutorial stays complete.
    let result2 = run_init(&inputs, &config_path).expect("re-init must succeed");
    assert!(result2.config_path.exists());
    assert!(
        is_tutorial_complete(&config_dir),
        "Tutorial completion must survive re-init"
    );
    assert!(
        !is_first_run(&config_dir),
        "Must still not be first run after re-init"
    );
}
