#![allow(unused_imports)]
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

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
pub struct AuthorizationCheckRequest {
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organization_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthorizationCheckResponse {
    pub allowed: bool,
    pub reason: String,
    pub policy_revision: String,
    pub consistency_token: Option<String>,
    pub resource_revision: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthorizationBatchCheckRequest {
    pub checks: Vec<AuthorizationCheckRequest>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthorizationBatchCheckResponse {
    pub results: Vec<AuthorizationCheckResponse>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthorizationCapabilitiesResponse {
    pub provider: String,
    pub batch_check: bool,
    pub list_resources: bool,
    pub consistency_tokens: bool,
    pub max_batch_checks: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdminUserSummary {
    pub user_id: String,
    pub primary_email: String,
    pub disabled: bool,
    pub email_verified: bool,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdminUserListResponse {
    pub users: Vec<AdminUserSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdminUserStatusRequest {
    pub user_id: String,
    pub disabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdminProviderRequest {
    pub provider_id: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyVersionSummary {
    pub version_id: String,
    pub status: String,
    pub policy_hash: String,
    pub published_by: String,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyVersionListResponse {
    pub versions: Vec<PolicyVersionSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyPublishRequest {
    pub policy_text: String,
    pub schema_text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HealthStatusResponse {
    pub status: String,
    pub storage_backend: String,
    pub mail_transport: String,
    pub authorization_provider: String,
    pub production_mode: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditEventSummary {
    pub sequence: u64,
    pub organization_id: Option<String>,
    pub actor_user_id: String,
    pub action: String,
    pub target_type: String,
    pub target_id: String,
    pub outcome: String,
    pub recorded_at_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditEventListResponse {
    pub events: Vec<AuditEventSummary>,
    pub next_cursor: u64,
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
