//! Plugin credential refresh protocol (v1.1 extension).
//!
//! Defines `CredentialRequest` (plugin -> host) and `CredentialResponse`
//! (host -> plugin) event types so plugins can request fresh CSP
//! credentials mid-execution without embedding long-lived secrets.

use serde::{Deserialize, Serialize};

/// A credential request from a plugin (v1.1 protocol extension).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CredentialRequest {
    #[serde(rename = "type")]
    pub event_type: String,
    pub provider: String,
    #[serde(default)]
    pub scopes: Vec<String>,
}

/// A credential response from the host to a plugin (v1.1 protocol extension).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CredentialResponse {
    #[serde(rename = "type")]
    pub event_type: String,
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_key_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_access_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl CredentialResponse {
    /// Build a GCP credential response with an OAuth2 access token.
    pub fn gcp(token: String, expires_in: u64) -> Self {
        Self {
            event_type: "credential_response".to_string(),
            provider: "gcp".to_string(),
            access_token: Some(token),
            access_key_id: None,
            secret_access_key: None,
            session_token: None,
            expires_in: Some(expires_in),
            error: None,
        }
    }

    /// Build an AWS credential response with STS temporary credentials.
    pub fn aws(
        access_key_id: String,
        secret_access_key: String,
        session_token: String,
        expires_in: u64,
    ) -> Self {
        Self {
            event_type: "credential_response".to_string(),
            provider: "aws".to_string(),
            access_token: None,
            access_key_id: Some(access_key_id),
            secret_access_key: Some(secret_access_key),
            session_token: Some(session_token),
            expires_in: Some(expires_in),
            error: None,
        }
    }

    /// Build an error response when credential refresh fails.
    pub fn error(provider: String, message: String) -> Self {
        Self {
            event_type: "credential_response".to_string(),
            provider,
            access_token: None,
            access_key_id: None,
            secret_access_key: None,
            session_token: None,
            expires_in: None,
            error: Some(message),
        }
    }
}

/// Try to parse a single NDJSON line as a credential request.
///
/// Returns `None` if the line is not valid JSON, or if the `type` field
/// is not `"credential_request"`.
pub fn try_parse_credential_request(line: &str) -> Option<CredentialRequest> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    if value.get("type")?.as_str()? == "credential_request" {
        serde_json::from_str(line).ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-INFRA-013
    #[test]
    fn test_parse_credential_request_gcp() {
        let line = r#"{"type":"credential_request","provider":"gcp","scopes":["https://www.googleapis.com/auth/cloud-platform"]}"#;
        let req = try_parse_credential_request(line).unwrap();
        assert_eq!(req.event_type, "credential_request");
        assert_eq!(req.provider, "gcp");
        assert_eq!(req.scopes.len(), 1);
        assert_eq!(
            req.scopes[0],
            "https://www.googleapis.com/auth/cloud-platform"
        );
    }

    // rtmx:req REQ-INFRA-013
    #[test]
    fn test_parse_credential_request_aws() {
        let line =
            r#"{"type":"credential_request","provider":"aws","scopes":["sts:AssumeRole"]}"#;
        let req = try_parse_credential_request(line).unwrap();
        assert_eq!(req.event_type, "credential_request");
        assert_eq!(req.provider, "aws");
        assert_eq!(req.scopes, vec!["sts:AssumeRole"]);
    }

    // rtmx:req REQ-INFRA-013
    #[test]
    fn test_parse_ignores_other_events() {
        let line =
            r#"{"type":"progress","resource":"kms","operation":"create","status":"complete"}"#;
        assert!(try_parse_credential_request(line).is_none());
    }

    // rtmx:req REQ-INFRA-013
    #[test]
    fn test_parse_ignores_malformed_json() {
        assert!(try_parse_credential_request("{not json}").is_none());
        assert!(try_parse_credential_request("").is_none());
        assert!(try_parse_credential_request("plain text").is_none());
    }

    // rtmx:req REQ-INFRA-013
    #[test]
    fn test_response_gcp_serializes() {
        let resp = CredentialResponse::gcp("ya29.token".to_string(), 3600);
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["type"], "credential_response");
        assert_eq!(json["provider"], "gcp");
        assert_eq!(json["access_token"], "ya29.token");
        assert_eq!(json["expires_in"], 3600);
        // AWS-specific fields should be absent (skip_serializing_if)
        assert!(json.get("access_key_id").is_none());
        assert!(json.get("secret_access_key").is_none());
        assert!(json.get("session_token").is_none());
        assert!(json.get("error").is_none());
    }

    // rtmx:req REQ-INFRA-013
    #[test]
    fn test_response_aws_serializes() {
        let resp = CredentialResponse::aws(
            "AKIAIOSFODNN7EXAMPLE".to_string(),
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
            "FwoGZXIvYXdzEBY...".to_string(),
            900,
        );
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["type"], "credential_response");
        assert_eq!(json["provider"], "aws");
        assert_eq!(json["access_key_id"], "AKIAIOSFODNN7EXAMPLE");
        assert_eq!(
            json["secret_access_key"],
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
        );
        assert_eq!(json["session_token"], "FwoGZXIvYXdzEBY...");
        assert_eq!(json["expires_in"], 900);
        // GCP-specific field should be absent
        assert!(json.get("access_token").is_none());
        assert!(json.get("error").is_none());
    }

    // rtmx:req REQ-INFRA-013
    #[test]
    fn test_response_error_serializes() {
        let resp =
            CredentialResponse::error("gcp".to_string(), "token refresh failed".to_string());
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["type"], "credential_response");
        assert_eq!(json["provider"], "gcp");
        assert_eq!(json["error"], "token refresh failed");
        // No credential fields
        assert!(json.get("access_token").is_none());
        assert!(json.get("access_key_id").is_none());
        assert!(json.get("secret_access_key").is_none());
        assert!(json.get("session_token").is_none());
        assert!(json.get("expires_in").is_none());
    }

    // rtmx:req REQ-INFRA-013
    #[test]
    fn test_credential_request_roundtrip() {
        let original = CredentialRequest {
            event_type: "credential_request".to_string(),
            provider: "gcp".to_string(),
            scopes: vec!["https://www.googleapis.com/auth/cloud-platform".to_string()],
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed = try_parse_credential_request(&json).unwrap();
        assert_eq!(original, parsed);
    }
}
