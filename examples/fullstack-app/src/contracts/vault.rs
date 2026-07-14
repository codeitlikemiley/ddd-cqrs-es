#![allow(unused_imports)]
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretCreateRequest {
    /// Env-like key: `^[A-Z][A-Z0-9_]{1,63}$` (preferred).
    #[serde(default)]
    pub key: String,
    /// Legacy alias for `key` / human label.
    #[serde(default)]
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub description: String,
    /// `user` (default) or `app` (platform features).
    #[serde(default = "default_vault_scope_user")]
    pub scope: String,
}

fn default_vault_scope_user() -> String {
    "user".to_owned()
}

/// Client-safe vault entry — **never** includes the secret value.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretSummary {
    pub id: String,
    /// Env-like key for connectors (`STRIPE_SECRET_KEY`).
    #[serde(default)]
    pub key: String,
    /// Display label.
    #[serde(default)]
    pub label: String,
    /// Backward-compat: same as `key` or `label`.
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_vault_scope_user")]
    pub scope: String,
    #[serde(default)]
    pub created_at_ms: u64,
    #[serde(default)]
    pub updated_at_ms: u64,
    /// Always the masked placeholder for UI.
    #[serde(default = "default_masked_secret")]
    pub masked_value: String,
}

fn default_masked_secret() -> String {
    "••••••••".to_owned()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretRevealResponse {
    pub id: String,
    pub key: String,
    /// Plaintext — only from dedicated reveal endpoint after authz.
    pub value: String,
    /// UI should remask after this many seconds.
    pub reveal_ttl_seconds: u32,
}

