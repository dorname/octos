//! OAuth PKCE and device code flows for OpenAI.

use std::collections::HashMap;

use chrono::Utc;
use eyre::{Result, WrapErr};
use reqwest::Client;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::store::AuthCredential;

// OpenAI OAuth configuration (public client — same as picoclaw/claude-code).
const OPENAI_ISSUER: &str = "https://auth.openai.com";
const OPENAI_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const REDIRECT_PORT: u16 = 1455;
const REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
// Pre-encoded for URL query string construction.
const REDIRECT_URI_ENCODED: &str = "http%3A%2F%2Flocalhost%3A1455%2Fauth%2Fcallback";
const OPENAI_SCOPES_ENCODED: &str = "openid%20profile%20email%20offline_access";

struct PkceChallenge {
    verifier: String,
    challenge: String,
}

/// Generate a PKCE code verifier and S256 challenge.
fn generate_pkce() -> PkceChallenge {
    // 64 random hex chars from two UUIDv4s (sufficient entropy for PKCE).
    let verifier = format!(
        "{}{}",
        uuid::Uuid::new_v4().as_simple(),
        uuid::Uuid::new_v4().as_simple(),
    );

    let hash = Sha256::digest(verifier.as_bytes());
    let challenge = base64_url_encode(&hash);

    PkceChallenge {
        verifier,
        challenge,
    }
}

/// Base64-URL encode without padding (RFC 7636).
fn base64_url_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

/// Generate a random state parameter.
fn random_state() -> String {
    uuid::Uuid::new_v4().as_simple().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_pkce_verifier_length() {
        let pkce = generate_pkce();
        // Two UUIDv4 simple strings = 32+32 = 64 hex chars
        assert_eq!(pkce.verifier.len(), 64);
    }

    #[test]
    fn test_generate_pkce_challenge_is_base64url() {
        let pkce = generate_pkce();
        // S256 challenge should be non-empty base64url (no padding)
        assert!(!pkce.challenge.is_empty());
        assert!(!pkce.challenge.contains('='));
        assert!(!pkce.challenge.contains('+'));
        assert!(!pkce.challenge.contains('/'));
        // SHA-256 output = 32 bytes -> base64url = 43 chars (without padding)
        assert_eq!(pkce.challenge.len(), 43);
    }

    #[test]
    fn test_generate_pkce_unique() {
        let p1 = generate_pkce();
        let p2 = generate_pkce();
        assert_ne!(p1.verifier, p2.verifier);
        assert_ne!(p1.challenge, p2.challenge);
    }

    #[test]
    fn test_base64_url_encode_known_value() {
        // SHA-256 of empty string
        let hash = sha2::Sha256::digest(b"");
        let encoded = base64_url_encode(&hash);
        assert_eq!(encoded, "47DEQpj8HBSa-_TImW-5JCeuQeRkm5NMpJWZG3hSuFU");
    }

    #[test]
    fn test_base64_url_encode_no_padding() {
        let encoded = base64_url_encode(b"test");
        assert!(!encoded.contains('='));
    }

    #[test]
    fn test_random_state_format() {
        let state = random_state();
        // UUIDv4 simple = 32 hex chars
        assert_eq!(state.len(), 32);
        assert!(state.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_random_state_unique() {
        let s1 = random_state();
        let s2 = random_state();
        assert_ne!(s1, s2);
    }

    #[test]
    fn test_deserialize_device_code_response() {
        let json = r#"{
            "device_auth_id": "deviceauth_abc123",
            "user_code": "ABCD-1234",
            "interval": "5",
            "expires_at": "2026-08-25T12:00:00+00:00"
        }"#;
        let device: DeviceCodeResponse = serde_json::from_str(json).expect("parses");
        assert_eq!(device.device_auth_id, "deviceauth_abc123");
        assert_eq!(device.user_code, "ABCD-1234");
        assert_eq!(device.interval, 5);
        assert!(device.expires_at.is_some());
        assert_eq!(
            device.verification_uri,
            "https://auth.openai.com/codex/device"
        );
    }

    #[test]
    fn test_deserialize_device_code_response_with_usercode_alias() {
        let json = r#"{
            "device_auth_id": "deviceauth_xyz789",
            "usercode": "WXYZ-5678",
            "interval": 5
        }"#;
        let device: DeviceCodeResponse = serde_json::from_str(json).expect("parses");
        assert_eq!(device.user_code, "WXYZ-5678");
        assert_eq!(device.interval, 5);
    }

    #[test]
    fn test_deserialize_code_success_response() {
        let json = r#"{
            "authorization_code": "auth_code_123",
            "code_verifier": "verifier_456"
        }"#;
        let code: CodeSuccessResponse = serde_json::from_str(json).expect("parses");
        assert_eq!(code.authorization_code, "auth_code_123");
        assert_eq!(code.code_verifier, "verifier_456");
    }

    /// Build an unsigned JWT string from a JSON payload (test helper).
    fn test_jwt(payload: &str) -> String {
        use base64::Engine;
        let h = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256"}"#);
        let p = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload);
        format!("{h}.{p}.fakesig")
    }

    #[test]
    fn should_extract_account_id_and_plan_from_oauth_jwt() {
        let token = test_jwt(
            r#"{"https://api.openai.com/auth":{"chatgpt_account_id":"acct-abc","chatgpt_plan_type":"plus"}}"#,
        );
        assert_eq!(chatgpt_account_id(&token).as_deref(), Some("acct-abc"));
        assert_eq!(chatgpt_plan_type(&token).as_deref(), Some("plus"));
    }

    #[test]
    fn should_return_none_for_opaque_api_key() {
        assert_eq!(chatgpt_account_id("sk-proj-abc123"), None);
        assert_eq!(chatgpt_plan_type("sk-proj-abc123"), None);
    }

    #[test]
    fn should_return_none_when_auth_claim_missing() {
        let token = test_jwt(r#"{"sub":"user-1","iss":"https://auth.openai.com"}"#);
        assert_eq!(chatgpt_account_id(&token), None);
        assert_eq!(chatgpt_plan_type(&token), None);
    }

    #[test]
    fn should_return_none_for_malformed_jwt_payload() {
        let token = "header.not-base64!!!.sig";
        assert_eq!(chatgpt_account_id(token), None);
    }

    #[test]
    fn should_merge_refreshed_token_into_existing_credential() {
        let old = AuthCredential {
            access_token: "old-jwt".into(),
            refresh_token: Some("old-refresh".into()),
            expires_at: Some(Utc::now() - chrono::Duration::minutes(5)),
            provider: "openai".into(),
            auth_method: "device_code".into(),
            account_id: None,
        };
        let new_jwt =
            test_jwt(r#"{"https://api.openai.com/auth":{"chatgpt_account_id":"acct-new"}}"#);
        let token = TokenResponse {
            access_token: new_jwt,
            refresh_token: Some("new-refresh".into()),
            expires_in: Some(3600),
        };
        let merged = merge_refreshed_credential(&old, token);
        assert_eq!(merged.provider, "openai");
        assert_eq!(merged.auth_method, "device_code");
        assert_eq!(merged.refresh_token.as_deref(), Some("new-refresh"));
        assert_eq!(merged.account_id.as_deref(), Some("acct-new"));
        assert!(!merged.is_expired());
    }

    #[test]
    fn should_keep_old_refresh_token_when_response_omits_one() {
        let old = AuthCredential {
            access_token: "old-jwt".into(),
            refresh_token: Some("old-refresh".into()),
            expires_at: None,
            provider: "openai".into(),
            auth_method: "oauth".into(),
            account_id: Some("acct-old".into()),
        };
        let token = TokenResponse {
            access_token: "new-jwt-not-really".into(),
            refresh_token: None,
            expires_in: Some(60),
        };
        let merged = merge_refreshed_credential(&old, token);
        assert_eq!(merged.refresh_token.as_deref(), Some("old-refresh"));
        assert_eq!(merged.access_token, "new-jwt-not-really");
    }
}

/// Run the browser-based OAuth PKCE flow for OpenAI.
pub async fn browser_oauth_flow() -> Result<AuthCredential> {
    let pkce = generate_pkce();
    let state = random_state();

    let auth_url = format!(
        "{}/authorize?client_id={}&redirect_uri={}&response_type=code&scope={}&code_challenge={}&code_challenge_method=S256&state={}",
        OPENAI_ISSUER,
        OPENAI_CLIENT_ID,
        REDIRECT_URI_ENCODED,
        OPENAI_SCOPES_ENCODED,
        pkce.challenge,
        state,
    );

    // Start listener before opening browser to avoid race.
    let listener = TcpListener::bind(format!("127.0.0.1:{REDIRECT_PORT}"))
        .await
        .wrap_err_with(|| format!("failed to bind port {REDIRECT_PORT} for OAuth callback"))?;

    println!("Opening browser for OpenAI login...");
    if open::that(&auth_url).is_err() {
        println!("Could not open browser. Please visit:\n{auth_url}");
    }

    // Wait for the callback.
    let code = wait_for_callback(&listener, &state).await?;

    // Exchange code for token.
    let token = exchange_code(&code, &pkce.verifier).await?;

    Ok(token_to_credential(token, "oauth"))
}

/// Run the device code OAuth flow for OpenAI.
pub async fn device_code_flow() -> Result<AuthCredential> {
    let client = Client::new();

    let resp = client
        .post(format!("{OPENAI_ISSUER}/api/accounts/deviceauth/usercode"))
        .json(&serde_json::json!({
            "client_id": OPENAI_CLIENT_ID,
        }))
        .send()
        .await
        .wrap_err("failed to request device code")?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        eyre::bail!("device code request failed: {body}");
    }

    let device: DeviceCodeResponse = resp.json().await?;
    let verification_uri = device.verification_uri;

    println!();
    println!("Go to: {verification_uri}");
    println!("Enter code: {}", device.user_code);
    println!();
    println!("Waiting for authorization...");

    let interval = std::time::Duration::from_secs(device.interval.max(5));
    let deadline = device
        .expires_at
        .unwrap_or_else(|| Utc::now() + chrono::Duration::minutes(15));

    loop {
        tokio::time::sleep(interval).await;

        if Utc::now() > deadline {
            eyre::bail!("device code authorization timed out");
        }

        let resp = client
            .post(format!("{OPENAI_ISSUER}/api/accounts/deviceauth/token"))
            .json(&serde_json::json!({
                "device_auth_id": &device.device_auth_id,
                "user_code": &device.user_code,
            }))
            .send()
            .await?;

        if resp.status().is_success() {
            let code_resp: CodeSuccessResponse = resp.json().await?;
            let token =
                exchange_device_code(&code_resp.authorization_code, &code_resp.code_verifier)
                    .await?;
            return Ok(token_to_credential(token, "device_code"));
        }

        // Check for pending vs actual error.
        let status = resp.status();
        let body: serde_json::Value = resp.json().await.unwrap_or_default();
        let error_code = body["error"]["code"].as_str().unwrap_or("");
        let error_msg = body["error"]["message"].as_str().unwrap_or("");

        if status == reqwest::StatusCode::FORBIDDEN
            || status == reqwest::StatusCode::NOT_FOUND
            || error_code == "deviceauth_authorization_pending"
            || error_msg == "authorization_pending"
            || error_msg == "slow_down"
        {
            continue;
        }

        eyre::bail!("device code auth failed: {}", body);
    }
}

/// Wait for OAuth callback on the local TCP listener.
async fn wait_for_callback(listener: &TcpListener, expected_state: &str) -> Result<String> {
    let (mut stream, _) = listener
        .accept()
        .await
        .wrap_err("failed to accept callback connection")?;

    let mut buf = vec![0u8; 4096];
    let n = stream.read(&mut buf).await?;
    let request = String::from_utf8_lossy(&buf[..n]);

    // Parse: GET /auth/callback?code=XXX&state=YYY HTTP/1.1
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .ok_or_else(|| eyre::eyre!("invalid callback request"))?;

    let query = path
        .split('?')
        .nth(1)
        .ok_or_else(|| eyre::eyre!("no query string in callback"))?;

    let params: HashMap<&str, &str> = query
        .split('&')
        .filter_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            Some((k, v))
        })
        .collect();

    let state = params
        .get("state")
        .ok_or_else(|| eyre::eyre!("no state in callback"))?;
    if *state != expected_state {
        eyre::bail!("OAuth state mismatch — possible CSRF attack");
    }

    let code = params
        .get("code")
        .ok_or_else(|| eyre::eyre!("no code in callback"))?
        .to_string();

    // Send HTML response back to browser.
    let html = "<html><body><h2>Login successful!</h2><p>You can close this tab.</p></body></html>";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        html.len(),
        html
    );
    stream.write_all(response.as_bytes()).await.ok();

    Ok(code)
}

/// Exchange authorization code for tokens.
async fn exchange_code(code: &str, verifier: &str) -> Result<TokenResponse> {
    let client = Client::new();
    let resp = client
        .post(format!("{OPENAI_ISSUER}/api/accounts/auth/token"))
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", REDIRECT_URI),
            ("client_id", OPENAI_CLIENT_ID),
            ("code_verifier", verifier),
        ])
        .send()
        .await
        .wrap_err("failed to exchange code for token")?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        eyre::bail!("token exchange failed: {body}");
    }

    resp.json().await.wrap_err("failed to parse token response")
}

/// Exchange a device-flow authorization code for tokens.
async fn exchange_device_code(code: &str, verifier: &str) -> Result<TokenResponse> {
    const DEVICE_REDIRECT_URI: &str = "https://auth.openai.com/deviceauth/callback";

    let client = Client::new();
    let resp = client
        .post(format!("{OPENAI_ISSUER}/oauth/token"))
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_id", OPENAI_CLIENT_ID),
            ("code_verifier", verifier),
            ("redirect_uri", DEVICE_REDIRECT_URI),
        ])
        .send()
        .await
        .wrap_err("failed to exchange device code for token")?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        eyre::bail!("device code token exchange failed: {body}");
    }

    resp.json()
        .await
        .wrap_err("failed to parse device token response")
}

/// Decode a JWT payload without verifying the signature. The token is the
/// user's own credential read back from the local store — the signature is
/// verified server-side every time the token is used.
fn parse_jwt_payload(token: &str) -> Option<serde_json::Value> {
    let payload = token.split('.').nth(1)?;
    use base64::Engine;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    serde_json::from_slice(&decoded).ok()
}

/// ChatGPT workspace/account ID (`chatgpt_account_id` claim) from an OpenAI
/// OAuth access token. The Codex backend requires it as the
/// `chatgpt-account-id` header. Returns `None` for opaque (non-JWT) tokens
/// such as platform API keys.
pub fn chatgpt_account_id(token: &str) -> Option<String> {
    let claims = parse_jwt_payload(token)?;
    claims["https://api.openai.com/auth"]["chatgpt_account_id"]
        .as_str()
        .map(str::to_string)
}

/// ChatGPT plan type (`chatgpt_plan_type` claim: "plus", "pro", ...) from an
/// OpenAI OAuth access token, for display in login/status output.
pub fn chatgpt_plan_type(token: &str) -> Option<String> {
    let claims = parse_jwt_payload(token)?;
    claims["https://api.openai.com/auth"]["chatgpt_plan_type"]
        .as_str()
        .map(str::to_string)
}

/// Merge a refresh response into the stored credential: new access token and
/// expiry, new refresh token when the server rotated one (otherwise keep the
/// old), `account_id` re-parsed from the new JWT.
fn merge_refreshed_credential(old: &AuthCredential, token: TokenResponse) -> AuthCredential {
    let account_id = chatgpt_account_id(&token.access_token).or_else(|| old.account_id.clone());
    let expires_at = token
        .expires_in
        .map(|secs| Utc::now() + chrono::Duration::seconds(secs as i64));
    AuthCredential {
        access_token: token.access_token,
        refresh_token: token.refresh_token.or_else(|| old.refresh_token.clone()),
        expires_at,
        provider: old.provider.clone(),
        auth_method: old.auth_method.clone(),
        account_id,
    }
}

/// Exchange a refresh token for a fresh access token (blocking).
///
/// Safe to call from ANY thread: `reqwest::blocking` panics when driven from
/// a tokio runtime thread, and credential-resolution callers can't always
/// know their thread — so the HTTP call runs on a dedicated OS thread (the
/// join only parks the caller; it never deadlocks the runtime). Refresh
/// happens at most once per token lifetime (~days), so the spawn cost is
/// irrelevant.
pub fn refresh_access_token_blocking(old: &AuthCredential) -> Result<AuthCredential> {
    let old = old.clone();
    std::thread::spawn(move || refresh_access_token_inner(&old))
        .join()
        .map_err(|_| eyre::eyre!("token refresh thread panicked"))?
}

fn refresh_access_token_inner(old: &AuthCredential) -> Result<AuthCredential> {
    let refresh_token = old
        .refresh_token
        .as_deref()
        .ok_or_else(|| eyre::eyre!("credential has no refresh token"))?;

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!("{OPENAI_ISSUER}/oauth/token"))
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", OPENAI_CLIENT_ID),
            ("refresh_token", refresh_token),
        ])
        .send()
        .wrap_err("failed to send token refresh request")?;

    if !resp.status().is_success() {
        let body = resp.text().unwrap_or_default();
        eyre::bail!("token refresh failed: {body}");
    }

    let token: TokenResponse = resp
        .json()
        .wrap_err("failed to parse refresh token response")?;
    Ok(merge_refreshed_credential(old, token))
}

fn token_to_credential(token: TokenResponse, auth_method: &str) -> AuthCredential {
    let account_id = chatgpt_account_id(&token.access_token);
    let expires_at = token
        .expires_in
        .map(|secs| Utc::now() + chrono::Duration::seconds(secs as i64));

    AuthCredential {
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        expires_at,
        provider: "openai".to_string(),
        auth_method: auth_method.to_string(),
        account_id,
    }
}

#[derive(serde::Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
}

#[derive(serde::Deserialize)]
struct DeviceCodeResponse {
    device_auth_id: String,
    #[serde(alias = "usercode")]
    user_code: String,
    #[serde(default = "default_verification_uri")]
    verification_uri: String,
    #[serde(
        default = "default_interval",
        deserialize_with = "deserialize_u64_from_string"
    )]
    interval: u64,
    #[serde(default)]
    expires_at: Option<chrono::DateTime<Utc>>,
}

#[derive(serde::Deserialize)]
struct CodeSuccessResponse {
    authorization_code: String,
    code_verifier: String,
}

fn default_verification_uri() -> String {
    "https://auth.openai.com/codex/device".to_string()
}

fn default_interval() -> u64 {
    5
}

fn deserialize_u64_from_string<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrU64 {
        String(String),
        U64(u64),
    }
    match StringOrU64::deserialize(deserializer)? {
        StringOrU64::String(s) => s.parse().map_err(serde::de::Error::custom),
        StringOrU64::U64(n) => Ok(n),
    }
}
