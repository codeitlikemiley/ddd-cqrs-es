#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
    pub tenant_id: Option<String>,
    pub user_id: Option<String>,
    pub primary_email: Option<String>,
    pub expires_at: Option<String>,
    pub permissions: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OAuthStartResponse {
    pub provider_id: String,
    pub authorization_url: String,
    pub state: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OAuthCallbackRequest {
    pub provider_id: String,
    pub code: Option<String>,
    pub state: Option<String>,
    pub redirect_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LoginCompletionResponse {
    pub authenticated: bool,
    pub redirect_url: String,
    pub session_id: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_in_seconds: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PasswordResetStartRequest {
    pub email: String,
    pub redirect_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PasswordResetStartResponse {
    pub accepted: bool,
    pub reset_url: Option<String>,
    pub expires_in_seconds: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PasswordResetCompleteRequest {
    pub token: String,
    pub password: String,
    pub redirect_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmailPasswordLoginRequest {
    pub email: String,
    pub password: String,
    pub redirect_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PasskeyStartResponse {
    pub challenge_id: String,
    pub public_key_options_json: String,
    pub redirect_url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PasskeyVerifyRequest {
    pub challenge_id: String,
    pub credential_json: String,
    pub redirect_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LogoutResponse {
    pub redirect_url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenRefreshResponse {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_in_seconds: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenRefreshRequest {
    pub refresh_token: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
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
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SigningKeySummary {
    pub kid: String,
    pub alg: String,
    pub status: String,
    pub active: bool,
    pub source: String,
    pub created_at_ms: Option<u64>,
    pub activated_at_ms: Option<u64>,
    pub retired_at_ms: Option<u64>,
    pub revoked_at_ms: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SigningKeyListResponse {
    pub keys: Vec<SigningKeySummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SigningKeyRotateRequest {
    #[serde(default)]
    pub admin_token: Option<String>,
    pub kid: String,
    #[serde(default)]
    pub retire_previous: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SigningKeyRotateResponse {
    pub active_kid: String,
    pub previous_kid: Option<String>,
    pub retired_previous: bool,
    pub keys: Vec<SigningKeySummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthzCheckRequest {
    pub tenant: String,
    pub subject: String,
    pub object: String,
    pub relation: String,
    #[serde(default)]
    pub context: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthzCheckResponse {
    pub allowed: bool,
    pub reason: String,
    pub model_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthzBatchCheckRequest {
    pub checks: Vec<AuthzCheckRequest>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthzBatchCheckResponse {
    pub results: Vec<AuthzCheckResponse>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthzListObjectsRequest {
    pub tenant: String,
    pub subject: String,
    pub relation: String,
    pub object_type: String,
    #[serde(default)]
    pub context: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthzListObjectsResponse {
    pub objects: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthzExpandRequest {
    pub tenant: String,
    pub object: String,
    pub relation: String,
    #[serde(default)]
    pub context: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthzExpandResponse {
    pub graph_json: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthzModelWriteRequest {
    pub model_id: String,
    pub schema_json: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthzModelReadResponse {
    pub model_id: String,
    pub schema_json: String,
    pub active: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthzModelWriteResponse {
    pub model_id: String,
    pub active: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RelationshipTupleWriteRequest {
    pub tuples_json: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RelationshipTupleWriteResponse {
    pub accepted: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageEventTypeCount {
    pub event_type: String,
    pub count: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageProjectionCheckpoint {
    pub projection_name: String,
    pub last_sequence: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageStatusResponse {
    pub event_count: u64,
    pub latest_sequence: u64,
    pub event_types: Vec<StorageEventTypeCount>,
    pub checkpoints: Vec<StorageProjectionCheckpoint>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageProjectionRunResponse {
    pub projection_name: String,
    pub last_sequence_before: u64,
    pub last_sequence_after: u64,
    pub events_scanned: u64,
    pub events_applied: u64,
    pub events_skipped: u64,
}
