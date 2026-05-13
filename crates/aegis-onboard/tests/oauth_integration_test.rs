//! Integration test: full PKCE OAuth flow against wiremock mock server.
//!
//! Exercises the complete lifecycle: PKCE generation -> auth URL -> callback
//! server -> token exchange -> refresh. Uses wiremock as a mock OAuth
//! authorization server.

use aegis_onboard::auth::{
    AuthConfig, TokenResponse, TokenState, build_auth_url, build_refresh_request,
    build_token_exchange_request, generate_pkce, generate_state, is_token_expired,
    run_callback_server,
};
use std::time::{Duration, Instant};
use wiremock::matchers::{body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Build an AuthConfig pointing at the mock server.
fn test_config(mock_url: &str) -> AuthConfig {
    AuthConfig {
        client_id: "aegis-test-client".to_string(),
        redirect_uri: "http://127.0.0.1:0/callback".to_string(),
        auth_endpoint: format!("{mock_url}/authorize"),
        scope: "openid profile".to_string(),
    }
}

/// Simulate a browser redirect hitting the callback server.
async fn simulate_redirect(port: u16, code: &str, state: &str) {
    use tokio::net::TcpStream;

    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}"))
        .await
        .expect("connect to callback server");

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let request = format!(
        "GET /callback?code={code}&state={state} HTTP/1.1\r\n\
         Host: 127.0.0.1:{port}\r\n\
         Connection: close\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .await
        .expect("write request");
    let mut buf = vec![0u8; 1024];
    let _ = stream.read(&mut buf).await;
}

// rtmx:req REQ-ONBOARD-034
#[tokio::test]
async fn test_pkce_flow_end_to_end() {
    // --- Step 1: Generate PKCE pair ---
    let (verifier, challenge) = generate_pkce();
    assert_eq!(verifier.len(), 128, "verifier must be 128 chars");
    assert_eq!(challenge.len(), 43, "S256 challenge must be 43 chars");

    // --- Step 2: Build authorization URL ---
    let mock_server = MockServer::start().await;
    let config = test_config(&mock_server.uri());
    let state = generate_state();
    let auth_url = build_auth_url(&config, &challenge, &state);

    // Verify URL structure
    assert!(auth_url.starts_with(&format!("{}/authorize?", mock_server.uri())));
    assert!(auth_url.contains("client_id=aegis-test-client"));
    assert!(auth_url.contains("code_challenge_method=S256"));
    assert!(auth_url.contains(&format!("state={state}")));
    assert!(auth_url.contains("response_type=code"));

    // --- Step 3: Start callback server and simulate redirect ---
    let state_for_server = state.clone();
    let (port, callback_fut) = run_callback_server(&state_for_server)
        .await
        .expect("callback server must start");

    let auth_code = "test-auth-code-12345";
    // Spawn the redirect simulation
    let state_for_redirect = state.clone();
    let redirect_handle =
        tokio::spawn(
            async move { simulate_redirect(port, auth_code, &state_for_redirect).await },
        );

    let callback = callback_fut.await.expect("callback must succeed");
    redirect_handle.await.expect("redirect task must complete");

    assert_eq!(callback.code, auth_code);

    // --- Step 4: Token exchange via wiremock ---
    let token_response = TokenResponse {
        access_token: "mock-access-token-xyz".to_string(),
        refresh_token: Some("mock-refresh-token-abc".to_string()),
        expires_in: 3600,
        token_type: "Bearer".to_string(),
    };

    Mock::given(method("POST"))
        .and(path("/token"))
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string_contains("grant_type=authorization_code"))
        .and(body_string_contains(format!("code={}", callback.code)))
        .and(body_string_contains(format!("code_verifier={verifier}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(&token_response))
        .expect(1)
        .mount(&mock_server)
        .await;

    let exchange_req = build_token_exchange_request(&config, &callback.code, &verifier);
    assert_eq!(exchange_req.grant_type, "authorization_code");
    assert_eq!(exchange_req.code, callback.code);
    assert_eq!(exchange_req.code_verifier, verifier);

    // Perform the actual HTTP exchange
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/token", mock_server.uri()))
        .header("content-type", "application/x-www-form-urlencoded")
        .body(exchange_req.to_form_body())
        .send()
        .await
        .expect("token exchange request must succeed");

    assert_eq!(resp.status(), 200);
    let body: TokenResponse = resp.json().await.expect("must parse token response");
    assert_eq!(body.access_token, "mock-access-token-xyz");
    assert_eq!(
        body.refresh_token.as_deref(),
        Some("mock-refresh-token-abc")
    );
    assert_eq!(body.token_type, "Bearer");

    // --- Step 5: Build token state and verify expiry ---
    let token_state = TokenState {
        access_token: body.access_token.clone(),
        refresh_token: body.refresh_token.clone(),
        expires_at: Instant::now() + Duration::from_secs(body.expires_in),
        token_type: body.token_type.clone(),
    };
    assert!(
        !is_token_expired(&token_state),
        "fresh token must not be expired"
    );

    // --- Step 6: Token refresh via wiremock ---
    let refresh_response = TokenResponse {
        access_token: "refreshed-access-token".to_string(),
        refresh_token: Some("new-refresh-token".to_string()),
        expires_in: 3600,
        token_type: "Bearer".to_string(),
    };

    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains("grant_type=refresh_token"))
        .and(body_string_contains("refresh_token=mock-refresh-token-abc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&refresh_response))
        .expect(1)
        .mount(&mock_server)
        .await;

    let refresh_req =
        build_refresh_request(&config.client_id, body.refresh_token.as_deref().unwrap());
    assert_eq!(refresh_req.grant_type, "refresh_token");
    assert_eq!(refresh_req.refresh_token, "mock-refresh-token-abc");

    // Perform the actual HTTP refresh
    let refresh_body = format!(
        "grant_type={}&refresh_token={}&client_id={}",
        refresh_req.grant_type, refresh_req.refresh_token, refresh_req.client_id
    );
    let resp = client
        .post(format!("{}/token", mock_server.uri()))
        .header("content-type", "application/x-www-form-urlencoded")
        .body(refresh_body)
        .send()
        .await
        .expect("refresh request must succeed");

    assert_eq!(resp.status(), 200);
    let refreshed: TokenResponse = resp.json().await.expect("must parse refresh response");
    assert_eq!(refreshed.access_token, "refreshed-access-token");
    assert_eq!(
        refreshed.refresh_token.as_deref(),
        Some("new-refresh-token")
    );

    // Verify all wiremock expectations met
    // (Mock::expect(1) will panic on drop if not satisfied)
}

// rtmx:req REQ-ONBOARD-034
#[tokio::test]
async fn test_pkce_flow_expired_token_triggers_refresh() {
    // Token that expired 10 minutes ago
    let expired_state = TokenState {
        access_token: "old-token".to_string(),
        refresh_token: Some("refresh-me".to_string()),
        expires_at: Instant::now() - Duration::from_secs(600),
        token_type: "Bearer".to_string(),
    };
    assert!(
        is_token_expired(&expired_state),
        "past-expiry token must be expired"
    );

    // Token expiring within the 5-minute buffer
    let soon_state = TokenState {
        access_token: "soon-token".to_string(),
        refresh_token: Some("refresh-me".to_string()),
        expires_at: Instant::now() + Duration::from_secs(60), // 1 min left, buffer is 5 min
        token_type: "Bearer".to_string(),
    };
    assert!(
        is_token_expired(&soon_state),
        "within-buffer token must be treated as expired"
    );
}

// rtmx:req REQ-ONBOARD-034
#[tokio::test]
async fn test_pkce_flow_callback_rejects_csrf() {
    let expected_state = "correct-state-value";
    let (port, callback_fut) = run_callback_server(expected_state)
        .await
        .expect("callback server must start");

    // Send request with wrong state
    let handle = tokio::spawn(async move {
        simulate_redirect(port, "auth-code", "wrong-state").await;
    });

    let result = callback_fut.await;
    handle.await.expect("redirect task must complete");

    assert!(result.is_err(), "mismatched state must be rejected");
    let err = result.unwrap_err();
    assert!(
        err.contains("state mismatch"),
        "error must mention state mismatch: {err}"
    );
}
