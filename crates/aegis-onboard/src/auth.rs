//! Authentication utilities for OAuth/OIDC flows.
//!
//! Provides PKCE (Proof Key for Code Exchange) challenge/verifier generation
//! per RFC 7636 for secure authorization code flows.

use sha2::{Digest, Sha256};

/// Unreserved URI characters allowed in PKCE verifiers (RFC 7636, Section 4.1).
const UNRESERVED: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";

/// Base64url alphabet (RFC 4648, Section 5).
const BASE64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Encode bytes as base64url without padding (RFC 4648, Section 5).
fn base64url_encode_no_pad(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(BASE64URL[((triple >> 18) & 0x3F) as usize] as char);
        out.push(BASE64URL[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(BASE64URL[((triple >> 6) & 0x3F) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(BASE64URL[(triple & 0x3F) as usize] as char);
        }
    }
    out
}

/// OAuth authorization server configuration.
pub struct AuthConfig {
    /// OAuth client ID.
    pub client_id: String,
    /// Redirect URI for the authorization callback.
    pub redirect_uri: String,
    /// Authorization endpoint URL.
    pub auth_endpoint: String,
    /// Requested OAuth scopes (space-separated).
    pub scope: String,
}

/// Result of a successful OAuth callback.
#[derive(Debug)]
pub struct AuthCallback {
    /// The authorization code returned by the identity provider.
    pub code: String,
    /// The state parameter echoed back for CSRF verification.
    pub state: String,
}

/// Percent-encode a string for use in URL query parameters.
///
/// Encodes all characters except unreserved characters (RFC 3986 Section 2.3).
fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for &b in input.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(char::from(HEX[(b >> 4) as usize]));
            out.push(char::from(HEX[(b & 0x0F) as usize]));
        }
    }
    out
}

/// Hex digits for percent-encoding.
const HEX: &[u8; 16] = b"0123456789ABCDEF";

/// Build an OAuth authorization URL with PKCE parameters.
///
/// Constructs a URL with query parameters: `client_id`, `redirect_uri`,
/// `code_challenge`, `code_challenge_method=S256`, `scope`, `state`,
/// `response_type=code`.
pub fn build_auth_url(config: &AuthConfig, challenge: &str, state: &str) -> String {
    format!(
        "{}?client_id={}&redirect_uri={}&code_challenge={}&code_challenge_method=S256\
         &scope={}&state={}&response_type=code",
        config.auth_endpoint,
        percent_encode(&config.client_id),
        percent_encode(&config.redirect_uri),
        percent_encode(challenge),
        percent_encode(&config.scope),
        percent_encode(state),
    )
}

/// Generate a random 32-character hex string for OAuth state (CSRF protection).
pub fn generate_state() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("OS RNG must be available");
    let mut hex = String::with_capacity(32);
    for b in bytes {
        hex.push(char::from(HEX[(b >> 4) as usize]));
        hex.push(char::from(HEX[(b & 0x0F) as usize]));
    }
    hex
}

/// Extract a query parameter value from a URL or path string.
///
/// Handles the `?key=value&key2=value2` format. Returns `None` if the key
/// is not found. Performs basic percent-decoding on the value.
pub fn extract_query_param(url: &str, key: &str) -> Option<String> {
    let query_start = url.find('?')?;
    let query = &url[query_start + 1..];
    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        let k = parts.next()?;
        let v = parts.next().unwrap_or("");
        if k == key {
            return Some(percent_decode(v));
        }
    }
    None
}

/// Basic percent-decoding for query parameter values.
fn percent_decode(input: &str) -> String {
    let mut out = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2]))
        {
            out.push((hi << 4) | lo);
            i += 3;
            continue;
        }
        // Treat '+' as space (form-encoded).
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Convert an ASCII hex character to its numeric value.
fn hex_val(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

/// Run a local HTTP callback server to receive the OAuth authorization code.
///
/// Binds to `127.0.0.1:0` (OS-assigned port) and waits for a single
/// `GET /callback?code=...&state=...` request. Verifies that the received
/// `state` matches `expected_state` to prevent CSRF attacks.
///
/// Returns `(port, future)` where `port` is the bound port and `future`
/// resolves to the [`AuthCallback`] once the request arrives.
pub async fn run_callback_server(
    expected_state: &str,
) -> Result<
    (
        u16,
        impl std::future::Future<Output = Result<AuthCallback, String>>,
    ),
    String,
> {
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("failed to bind callback server: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("failed to get local addr: {e}"))?
        .port();

    let expected = expected_state.to_owned();
    let fut = async move {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let (mut stream, _addr) = listener
            .accept()
            .await
            .map_err(|e| format!("accept failed: {e}"))?;

        let mut buf = vec![0u8; 4096];
        let n = stream
            .read(&mut buf)
            .await
            .map_err(|e| format!("read failed: {e}"))?;
        let request = String::from_utf8_lossy(&buf[..n]);

        // Parse the request line: GET /callback?code=...&state=... HTTP/1.1
        let first_line = request
            .lines()
            .next()
            .ok_or_else(|| "empty request".to_owned())?;
        let path = first_line
            .split_whitespace()
            .nth(1)
            .ok_or_else(|| "malformed request line".to_owned())?;

        let code = extract_query_param(path, "code")
            .ok_or_else(|| "missing 'code' query parameter".to_owned())?;
        let state = extract_query_param(path, "state")
            .ok_or_else(|| "missing 'state' query parameter".to_owned())?;

        if state != expected {
            let body = "Error: state mismatch (possible CSRF attack).";
            let response = format!(
                "HTTP/1.1 400 Bad Request\r\n\
                 Content-Length: {}\r\n\
                 Connection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes()).await;
            return Err(format!(
                "state mismatch: expected '{expected}', got '{state}'"
            ));
        }

        let body = "Authorization successful. You may close this window.";
        let response = format!(
            "HTTP/1.1 200 OK\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes()).await;

        Ok(AuthCallback { code, state })
    };

    Ok((port, fut))
}

/// Generate a PKCE code verifier and challenge pair per RFC 7636.
///
/// Returns `(verifier, challenge)` where:
/// - `verifier`: 128-character random string using unreserved URI characters
/// - `challenge`: `BASE64URL(SHA256(verifier))` without padding
///
/// # Panics
///
/// Panics if the OS random number generator fails.
pub fn generate_pkce() -> (String, String) {
    // Generate random bytes for the verifier. We need 128 characters,
    // each selected from the UNRESERVED set via modular reduction.
    let mut random_bytes = [0u8; 128];
    getrandom::fill(&mut random_bytes).expect("OS RNG must be available");

    let verifier: String = random_bytes
        .iter()
        .map(|&b| UNRESERVED[(b as usize) % UNRESERVED.len()] as char)
        .collect();

    // challenge = BASE64URL(SHA256(verifier)) without padding
    let hash = Sha256::digest(verifier.as_bytes());
    let challenge = base64url_encode_no_pad(&hash);

    (verifier, challenge)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    // rtmx:req REQ-ONBOARD-029
    #[test]
    fn test_pkce_challenge_verifier_pair() {
        let (verifier, challenge) = generate_pkce();

        // Verifier must be 43-128 characters (RFC 7636 Section 4.1).
        assert!(
            (43..=128).contains(&verifier.len()),
            "verifier length {} not in 43..=128",
            verifier.len()
        );

        // Verifier must only contain unreserved URI characters.
        for ch in verifier.chars() {
            assert!(
                ch.is_ascii_alphanumeric() || matches!(ch, '-' | '.' | '_' | '~'),
                "verifier contains invalid character: {ch:?}"
            );
        }

        // Challenge must be valid base64url (no padding).
        for ch in challenge.chars() {
            assert!(
                ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'),
                "challenge contains invalid base64url character: {ch:?}"
            );
        }
        assert!(
            !challenge.contains('='),
            "challenge must not contain padding"
        );

        // Challenge must equal BASE64URL(SHA256(verifier)).
        let expected_hash = Sha256::digest(verifier.as_bytes());
        let expected_challenge = base64url_encode_no_pad(&expected_hash);
        assert_eq!(
            challenge, expected_challenge,
            "challenge must be BASE64URL(SHA256(verifier))"
        );

        // SHA-256 produces 32 bytes -> 43 base64url characters (no padding).
        assert_eq!(challenge.len(), 43, "SHA-256 base64url should be 43 chars");
    }

    // rtmx:req REQ-ONBOARD-029
    #[test]
    fn test_pkce_verifiers_are_unique() {
        let (v1, _) = generate_pkce();
        let (v2, _) = generate_pkce();
        assert_ne!(v1, v2, "successive verifiers must differ");
    }

    // rtmx:req REQ-ONBOARD-029
    #[test]
    fn test_pkce_base64url_no_standard_chars() {
        // Ensure challenge uses URL-safe alphabet (- and _ instead of + and /).
        for _ in 0..10 {
            let (_, challenge) = generate_pkce();
            assert!(!challenge.contains('+'), "must use - not +");
            assert!(!challenge.contains('/'), "must use _ not /");
        }
    }

    // rtmx:req REQ-ONBOARD-030
    #[test]
    fn test_auth_url_contains_pkce_params() {
        let config = AuthConfig {
            client_id: "my-client".into(),
            redirect_uri: "http://127.0.0.1:8080/callback".into(),
            auth_endpoint: "https://auth.example.com/authorize".into(),
            scope: "openid profile".into(),
        };
        let challenge = "test_challenge_value";
        let state = "abc123def456";
        let url = build_auth_url(&config, challenge, state);

        assert!(url.starts_with("https://auth.example.com/authorize?"));
        assert!(url.contains("client_id=my-client"));
        assert!(
            url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A8080%2Fcallback"),
            "redirect_uri not properly encoded in: {url}"
        );
        assert!(url.contains("code_challenge=test_challenge_value"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("scope=openid%20profile"));
        assert!(url.contains("state=abc123def456"));
        assert!(url.contains("response_type=code"));
    }

    // rtmx:req REQ-ONBOARD-030
    #[test]
    fn test_auth_url_encodes_special_characters() {
        let config = AuthConfig {
            client_id: "client with spaces&more".into(),
            redirect_uri: "http://localhost/cb?extra=1".into(),
            auth_endpoint: "https://auth.example.com/authorize".into(),
            scope: "scope/special".into(),
        };
        let url = build_auth_url(&config, "chal", "st");

        assert!(
            url.contains("client_id=client%20with%20spaces%26more"),
            "client_id not properly encoded in: {url}"
        );
        assert!(
            url.contains("redirect_uri=http%3A%2F%2Flocalhost%2Fcb%3Fextra%3D1"),
            "redirect_uri not properly encoded in: {url}"
        );
        assert!(
            url.contains("scope=scope%2Fspecial"),
            "scope not properly encoded in: {url}"
        );
    }

    // rtmx:req REQ-ONBOARD-030
    #[test]
    fn test_state_is_32_hex_chars() {
        for _ in 0..10 {
            let state = generate_state();
            assert_eq!(state.len(), 32, "state must be 32 characters");
            assert!(
                state.chars().all(|c| c.is_ascii_hexdigit()),
                "state must contain only hex digits, got: {state}"
            );
        }
    }

    // rtmx:req REQ-ONBOARD-030
    #[test]
    fn test_states_are_unique() {
        let s1 = generate_state();
        let s2 = generate_state();
        assert_ne!(s1, s2, "successive states must differ");
    }

    // rtmx:req REQ-ONBOARD-031
    #[tokio::test]
    async fn test_callback_server_receives_code() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpStream;

        let expected_state = "test_state_value_123";
        let (port, server_fut) = run_callback_server(expected_state).await.unwrap();

        // Spawn the server in the background.
        let handle = tokio::spawn(server_fut);

        // Send a raw HTTP GET request to the callback server.
        let mut stream = TcpStream::connect(format!("127.0.0.1:{port}"))
            .await
            .expect("connect to callback server");
        let request = format!(
            "GET /callback?code=auth_code_xyz&state={expected_state} HTTP/1.1\r\n\
             Host: 127.0.0.1:{port}\r\n\
             Connection: close\r\n\r\n"
        );
        stream.write_all(request.as_bytes()).await.unwrap();

        // Read the response.
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        let response_str = String::from_utf8_lossy(&response);
        assert!(
            response_str.contains("200 OK"),
            "expected 200 OK, got: {response_str}"
        );

        // Verify the callback result.
        let callback = handle.await.unwrap().expect("server should succeed");
        assert_eq!(callback.code, "auth_code_xyz");
        assert_eq!(callback.state, expected_state);
    }

    // rtmx:req REQ-ONBOARD-031
    #[tokio::test]
    async fn test_callback_server_state_mismatch() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpStream;

        let expected_state = "correct_state";
        let (port, server_fut) = run_callback_server(expected_state).await.unwrap();

        let handle = tokio::spawn(server_fut);

        let mut stream = TcpStream::connect(format!("127.0.0.1:{port}"))
            .await
            .expect("connect to callback server");
        let request = format!(
            "GET /callback?code=some_code&state=wrong_state HTTP/1.1\r\n\
             Host: 127.0.0.1:{port}\r\n\
             Connection: close\r\n\r\n"
        );
        stream.write_all(request.as_bytes()).await.unwrap();

        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        let response_str = String::from_utf8_lossy(&response);
        assert!(
            response_str.contains("400 Bad Request"),
            "expected 400, got: {response_str}"
        );

        let result = handle.await.unwrap();
        assert!(result.is_err(), "state mismatch should return error");
        let err = result.unwrap_err();
        assert!(
            err.contains("state mismatch"),
            "error should mention state mismatch: {err}"
        );
    }

    // rtmx:req REQ-ONBOARD-031
    #[test]
    fn test_extract_query_param() {
        assert_eq!(
            extract_query_param("/callback?code=abc&state=xyz", "code"),
            Some("abc".into())
        );
        assert_eq!(
            extract_query_param("/callback?code=abc&state=xyz", "state"),
            Some("xyz".into())
        );
        assert_eq!(
            extract_query_param("/callback?code=abc&state=xyz", "missing"),
            None
        );
        assert_eq!(extract_query_param("/callback", "code"), None);
        // Percent-encoded value.
        assert_eq!(
            extract_query_param("/cb?val=hello%20world", "val"),
            Some("hello world".into())
        );
    }
}
