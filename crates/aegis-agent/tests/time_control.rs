//! Demonstrates tokio::time::pause() for deterministic timeout testing.
//!
//! With `start_paused = true`, the tokio runtime uses a simulated clock.
//! `time::advance()` moves the clock forward without wall-clock delay,
//! enabling tests of retry delays and timeouts that complete instantly.

use std::time::Duration;
use tokio::time;

// @req REQ-TEST-012
#[tokio::test(start_paused = true)]
async fn time_advance_is_deterministic() {
    // With time paused, advance() is instantaneous from wall-clock perspective.
    let start = time::Instant::now();
    time::advance(Duration::from_secs(60)).await;
    let elapsed = start.elapsed();
    assert!(
        elapsed >= Duration::from_secs(60),
        "tokio time must advance by the requested amount"
    );
    // This test completes instantly despite a 60-second logical advance.
}

// @req REQ-TEST-012
#[tokio::test(start_paused = true)]
async fn timeout_fires_without_wall_clock_sleep() {
    let result = time::timeout(Duration::from_secs(5), async {
        // Simulate a long operation that never completes in time.
        time::sleep(Duration::from_secs(3600)).await;
        "completed"
    })
    .await;
    assert!(
        result.is_err(),
        "timeout must fire at 5s even though task sleeps for 3600s"
    );
}

// @req REQ-TEST-012
#[tokio::test(start_paused = true)]
async fn retry_delay_sequence_completes_instantly() {
    // Simulate the exponential back-off delay pattern from aegis-agent::retry.
    // With time paused, the total 15-second delay sequence is instantaneous.
    let delays = [
        Duration::from_millis(1_000),
        Duration::from_millis(2_000),
        Duration::from_millis(4_000),
        Duration::from_millis(8_000),
    ];
    let start = time::Instant::now();
    for delay in &delays {
        time::sleep(*delay).await;
    }
    let elapsed = start.elapsed();
    let expected: Duration = delays.iter().sum();
    assert_eq!(
        elapsed, expected,
        "total elapsed must equal sum of all delays"
    );
}

// @req REQ-TEST-012
#[tokio::test(start_paused = true)]
async fn sleep_completes_only_after_sufficient_advance() {
    let sleep_fut = time::sleep(Duration::from_secs(10));
    tokio::pin!(sleep_fut);

    // Advance less than the sleep duration -- sleep must not complete.
    time::advance(Duration::from_secs(5)).await;
    let poll_result = tokio::time::timeout(Duration::from_millis(0), &mut sleep_fut).await;
    assert!(
        poll_result.is_err(),
        "sleep must not complete before its duration"
    );

    // Advance past the remaining duration -- sleep must complete.
    time::advance(Duration::from_secs(6)).await;
    let poll_result = tokio::time::timeout(Duration::from_millis(0), &mut sleep_fut).await;
    assert!(
        poll_result.is_ok(),
        "sleep must complete after sufficient time has passed"
    );
}
