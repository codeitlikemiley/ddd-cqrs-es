#![allow(unused_imports)]
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

macro_rules! redacted_debug {
    ($type:ident, visible [$($visible:ident),* $(,)?], secret [$($secret:ident),* $(,)?]) => {
        impl std::fmt::Debug for $type {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let mut debug = formatter.debug_struct(stringify!($type));
                $(debug.field(stringify!($visible), &self.$visible);)*
                $(debug.field(stringify!($secret), &"[REDACTED]");)*
                debug.finish()
            }
        }
    };
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthProviderSummary {
    pub provider_id: String,
    pub display_name: String,
    pub login_url: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthCapabilities {
    pub password_enabled: bool,
    pub oauth_enabled: bool,
    pub passkeys_enabled: bool,
    pub providers: Vec<AuthProviderSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionView {
    pub authenticated: bool,
    pub session_id: Option<String>,
    pub tenant_id: Option<String>,
    pub user_id: Option<String>,
    pub primary_email: Option<String>,
    pub expires_at: Option<String>,
    pub permissions: Vec<String>,
    pub assurance: String,
    pub system_administrator: bool,
    pub issued_at_unix_seconds: Option<u64>,
    pub expires_at_unix_seconds: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CsrfTokenResponse {
    pub token: String,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OAuthStartResponse {
    pub provider_id: String,
    pub authorization_url: String,
    pub state: String,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OAuthCallbackRequest {
    pub provider_id: String,
    pub code: Option<String>,
    pub state: Option<String>,
    pub redirect_url: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LoginCompletionResponse {
    pub authenticated: bool,
    pub redirect_url: String,
    pub session_id: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_in_seconds: u64,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PasswordResetStartRequest {
    pub email: String,
    pub redirect_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PasswordResetStartResponse {
    pub accepted: bool,
    pub expires_in_seconds: u64,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapturedMailResponse {
    pub message_kind: String,
    pub recipient: String,
    pub subject: String,
    /// Full plain-text body (greeting, CTA URL, security footer).
    pub body_text: String,
    /// Optional HTML multipart body for productized mail.
    #[serde(default)]
    pub body_html: Option<String>,
    /// One-time action URL extracted for capture-mode deep links.
    #[serde(default)]
    pub action_url: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmailVerificationCompleteRequest {
    pub token: String,
    pub redirect_url: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmailVerificationResendRequest {
    pub email: String,
    pub redirect_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AcceptedResponse {
    pub accepted: bool,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PasswordResetCompleteRequest {
    pub token: String,
    pub password: String,
    pub redirect_url: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmailPasswordLoginRequest {
    pub email: String,
    pub password: String,
    pub redirect_url: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmailPasswordRegisterRequest {
    pub email: String,
    pub password: String,
    pub redirect_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PasskeyStartRequest {
    pub email: Option<String>,
    pub redirect_url: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PasskeyStartResponse {
    pub challenge_id: String,
    pub public_key_options_json: String,
    pub redirect_url: String,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PasskeyVerifyRequest {
    pub challenge_id: String,
    pub credential_json: String,
    pub redirect_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LogoutResponse {
    pub redirect_url: String,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenRefreshResponse {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_in_seconds: u64,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenRefreshRequest {
    pub refresh_token: String,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenVerifyRequest {
    pub access_token: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenVerifyResponse {
    pub active: bool,
    pub subject: String,
    pub tenant_id: Option<String>,
    pub session_id: Option<String>,
    pub expires_at: u64,
    pub scopes: Vec<String>,
    #[serde(skip)]
    pub role_ids: Vec<String>,
    #[serde(skip)]
    pub policy_revision: Option<String>,
    pub assurance: String,
    pub system_administrator: bool,
    pub issued_at_unix_seconds: u64,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PasswordChangeRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountSessionSummary {
    pub session_id: String,
    pub organization_id: Option<String>,
    pub assurance: String,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub current: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountSessionListResponse {
    pub sessions: Vec<AccountSessionSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionRevokeRequest {
    pub session_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MfaStatusResponse {
    pub totp_enrolled: bool,
    pub recovery_codes_remaining: u32,
    pub assurance: String,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MfaEnrollStartResponse {
    pub credential_id: String,
    pub secret_base32: String,
    pub provisioning_uri: String,
}

impl std::fmt::Debug for MfaEnrollStartResponse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MfaEnrollStartResponse")
            .field("credential_id", &self.credential_id)
            .field("secret_base32", &"[REDACTED]")
            .field("provisioning_uri", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MfaCodeRequest {
    pub code: String,
}

impl std::fmt::Debug for MfaCodeRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MfaCodeRequest")
            .field("code", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MfaEnrollConfirmResponse {
    pub recovery_codes: Vec<String>,
    pub assurance: String,
}

impl std::fmt::Debug for MfaEnrollConfirmResponse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MfaEnrollConfirmResponse")
            .field("recovery_codes", &"[REDACTED]")
            .field("assurance", &self.assurance)
            .finish()
    }
}

redacted_debug!(OAuthStartResponse, visible [provider_id], secret [authorization_url, state]);
redacted_debug!(OAuthCallbackRequest, visible [provider_id, redirect_url], secret [code, state]);
redacted_debug!(LoginCompletionResponse, visible [authenticated, redirect_url, expires_in_seconds], secret [session_id, access_token, refresh_token]);
redacted_debug!(
    PasswordResetStartRequest,
    visible[redirect_url],
    secret[email]
);
redacted_debug!(CapturedMailResponse, visible [message_kind, subject], secret [recipient, body_text, body_html, action_url]);
redacted_debug!(
    EmailVerificationCompleteRequest,
    visible[redirect_url],
    secret[token]
);
redacted_debug!(
    EmailVerificationResendRequest,
    visible[redirect_url],
    secret[email]
);
redacted_debug!(PasswordResetCompleteRequest, visible [redirect_url], secret [token, password]);
redacted_debug!(EmailPasswordLoginRequest, visible [redirect_url], secret [email, password]);
redacted_debug!(EmailPasswordRegisterRequest, visible [redirect_url], secret [email, password]);
redacted_debug!(PasskeyStartResponse, visible [redirect_url], secret [challenge_id, public_key_options_json]);
redacted_debug!(PasskeyVerifyRequest, visible [redirect_url], secret [challenge_id, credential_json]);
redacted_debug!(TokenRefreshResponse, visible [expires_in_seconds], secret [access_token, refresh_token]);
redacted_debug!(TokenRefreshRequest, visible [], secret [refresh_token]);
redacted_debug!(TokenVerifyRequest, visible [], secret [access_token]);
redacted_debug!(PasswordChangeRequest, visible [], secret [current_password, new_password]);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mfa_enrollment_and_code_debug_output_is_redacted() {
        let enrollment = MfaEnrollStartResponse {
            credential_id: "totp-one".to_owned(),
            secret_base32: "TOPSECRETBASE32".to_owned(),
            provisioning_uri: "otpauth://secret".to_owned(),
        };
        let confirmation = MfaEnrollConfirmResponse {
            recovery_codes: vec!["AAAA-BBBB-CCCC-DDDD".to_owned()],
            assurance: "aal2".to_owned(),
        };
        let request = MfaCodeRequest {
            code: "123456".to_owned(),
        };

        let debug = format!("{enrollment:?} {confirmation:?} {request:?}");
        assert!(!debug.contains("TOPSECRETBASE32"));
        assert!(!debug.contains("otpauth://secret"));
        assert!(!debug.contains("AAAA-BBBB"));
        assert!(!debug.contains("123456"));
        assert!(debug.contains("[REDACTED]"));
    }

    #[test]
    fn password_and_token_contract_debug_output_is_redacted() {
        let login = EmailPasswordLoginRequest {
            email: "person@example.com".to_owned(),
            password: "correct horse battery staple".to_owned(),
            redirect_url: Some("/organizations".to_owned()),
        };
        let reset = PasswordResetCompleteRequest {
            token: "one-time-reset-token".to_owned(),
            password: "another correct password".to_owned(),
            redirect_url: None,
        };

        let debug = format!("{login:?} {reset:?}");

        assert!(!debug.contains("person@example.com"));
        assert!(!debug.contains("correct horse"));
        assert!(!debug.contains("one-time-reset-token"));
        assert!(debug.contains("[REDACTED]"));
    }
}
