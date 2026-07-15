//! Product storage adapters (KV, SQL, vault, dashboard data).
#![allow(unused_imports)]
#![allow(dead_code)]

use std::borrow::Cow;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "mail-capture")]
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use futures::lock::Mutex;
use serde_json::Value;
#[cfg(feature = "mail-capture")]
use serde_json::json;
use sha2::{Digest, Sha256};
#[cfg(all(feature = "spicedb", runtime_spin))]
use wasi_auth::authorization::{
    AccessRequest as SpiceDbAccessRequest, ActionName as SpiceDbActionName,
    Authorizer as SpiceDbAuthorizer, ConsistencyRequirement, Decision as AuthorizationDecision,
    Resource as SpiceDbResource, ResourceType as SpiceDbResourceType,
};
#[cfg(all(feature = "spicedb", runtime_spin))]
use wasi_auth::context::OrganizationId;
#[cfg(all(feature = "spicedb", runtime_spin))]
use wasi_auth::postgres::outbox::load_relationship_consistency;
use wasi_auth::schema::{AppliedSchemaMigration, plan_schema};
#[cfg(all(feature = "spicedb", runtime_spin))]
use wasi_auth::spicedb::{
    PermissionMap, SpiceDbBearerToken, SpiceDbEndpoint, SpiceDbProvider, SpiceDbTransport,
};

use crate::contracts::{
    HealthStatusResponse, StorageEventTypeCount, StorageProjectionRunResponse,
    StorageStatusResponse,
};
use crate::error::{AuthStackError, AuthStackResult};

use super::*;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct StoredSecret {
    pub(crate) id: String,
    /// Env-like key.
    #[serde(default)]
    pub(crate) key: String,
    /// Human label.
    #[serde(default)]
    pub(crate) label: String,
    /// Legacy name field (older payloads).
    #[serde(default)]
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) description: String,
    #[serde(default = "default_secret_scope")]
    pub(crate) scope: String,
    /// Legacy plaintext (migrated away on load; never persisted after migration).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) value: String,
    #[serde(default)]
    pub(crate) ciphertext_b64: String,
    #[serde(default)]
    pub(crate) nonce_b64: String,
    /// Unused (AES-GCM tag is part of ciphertext); kept for forward-compat.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) mac_b64: String,
    #[serde(default)]
    pub(crate) key_version: String,
    #[serde(default)]
    pub(crate) created_at_ms: u64,
    #[serde(default)]
    pub(crate) updated_at_ms: u64,
}

pub(crate) fn default_secret_scope() -> String {
    "user".to_owned()
}

pub(crate) const VAULT_SECRET_AAD_PREFIX: &[u8] = b"fullstack-app:vault-secret:v1:org:";
pub(crate) const VAULT_NONCE_BYTES: usize = 12;
pub(crate) const MAX_VAULT_SECRETS: usize = 64;
pub(crate) const MAX_VAULT_VALUE_BYTES: usize = 8_192;
pub(crate) const VAULT_REVEAL_TTL_SECONDS: u32 = 30;

/// Env-like vault key: `^[A-Z][A-Z0-9_]{1,63}$`.
pub fn validate_vault_secret_key(key: &str) -> AuthStackResult<()> {
    let key = key.trim();
    if key.len() < 2 || key.len() > 64 {
        return Err(AuthStackError::validation(
            "secret key must be 2–64 characters (e.g. STRIPE_SECRET_KEY)",
        ));
    }
    let mut chars = key.chars();
    let Some(first) = chars.next() else {
        return Err(AuthStackError::validation("secret key is required"));
    };
    if !first.is_ascii_uppercase() {
        return Err(AuthStackError::validation(
            "secret key must start with A–Z (e.g. API_TOKEN)",
        ));
    }
    if !chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') {
        return Err(AuthStackError::validation(
            "secret key may only contain A–Z, 0–9, and underscore",
        ));
    }
    Ok(())
}

pub(crate) fn vault_secret_aad(org_id: &str) -> Vec<u8> {
    let mut aad = Vec::with_capacity(VAULT_SECRET_AAD_PREFIX.len() + org_id.len());
    aad.extend_from_slice(VAULT_SECRET_AAD_PREFIX);
    aad.extend_from_slice(org_id.as_bytes());
    aad
}

pub(crate) async fn dashboard_vault_key_material() -> AuthStackResult<(String, [u8; 32])> {
    let production = store_config_value(AUTH_PRODUCTION_MODE)
        .await
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false);
    let configured = store_config_value(MFA_VAULT_KEY)
        .await
        .filter(|value| !value.trim().is_empty());
    let key: [u8; 32] = match configured {
        Some(encoded) => STANDARD
            .decode(encoded.trim())
            .ok()
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or_else(|| {
                AuthStackError::configuration("AUTH_VAULT_KEY_BASE64 must decode to 32 bytes")
            })?,
        None if production => {
            return Err(AuthStackError::configuration(
                "AUTH_VAULT_KEY_BASE64 is required in production",
            ));
        }
        None => Sha256::digest(b"fullstack-development-outbox-key").into(),
    };
    let key_version = store_config_value("AUTH_VAULT_KEY_VERSION")
        .await
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "development-v1".to_owned());
    Ok((key_version, key))
}

pub(crate) async fn dashboard_vault_key_ring()
-> AuthStackResult<(String, std::collections::BTreeMap<String, [u8; 32]>)> {
    let (active_version, active_key) = dashboard_vault_key_material().await?;
    let mut ring = std::collections::BTreeMap::new();
    if let Some(raw) = store_config_value("AUTH_VAULT_KEY_RING_JSON")
        .await
        .filter(|value| !value.trim().is_empty())
    {
        let configured: std::collections::BTreeMap<String, String> = serde_json::from_str(&raw)
            .map_err(|_| {
                AuthStackError::configuration(
                    "AUTH_VAULT_KEY_RING_JSON must be a JSON object of version to base64 key",
                )
            })?;
        for (version, encoded) in configured {
            if version.trim().is_empty() || version.len() > 128 {
                return Err(AuthStackError::configuration(
                    "AUTH_VAULT_KEY_RING_JSON contains an invalid key version",
                ));
            }
            let key: [u8; 32] = STANDARD
                .decode(encoded.trim())
                .ok()
                .and_then(|bytes| bytes.try_into().ok())
                .ok_or_else(|| {
                    AuthStackError::configuration(
                        "every AUTH_VAULT_KEY_RING_JSON key must decode to 32 bytes",
                    )
                })?;
            ring.insert(version, key);
        }
    }
    ring.insert(active_version.clone(), active_key);
    Ok((active_version, ring))
}

pub(crate) fn encrypt_vault_value(
    org_id: &str,
    plaintext: &str,
    key: &[u8; 32],
) -> AuthStackResult<(String, String)> {
    use aes_gcm::{
        Aes256Gcm, Nonce,
        aead::{Aead, KeyInit, Payload},
    };
    let mut nonce = [0_u8; VAULT_NONCE_BYTES];
    getrandom::getrandom(&mut nonce).map_err(|error| {
        AuthStackError::store(format!("vault nonce generation failed: {error}"))
    })?;
    let cipher = Aes256Gcm::new_from_slice(key.as_slice())
        .map_err(|_| AuthStackError::configuration("vault encryption key is invalid"))?;
    let aad = vault_secret_aad(org_id);
    let ciphertext = cipher
        .encrypt(
            &Nonce::from(nonce),
            Payload {
                msg: plaintext.as_bytes(),
                aad: &aad,
            },
        )
        .map_err(|_| AuthStackError::store("vault encryption failed"))?;
    Ok((STANDARD.encode(nonce), STANDARD.encode(ciphertext)))
}

pub(crate) fn decrypt_vault_value(
    org_id: &str,
    secret: &StoredSecret,
    key: &[u8; 32],
) -> AuthStackResult<String> {
    use aes_gcm::{
        Aes256Gcm, Nonce,
        aead::{Aead, KeyInit, Payload},
    };
    if secret.ciphertext_b64.is_empty() || secret.nonce_b64.is_empty() {
        if !secret.value.is_empty() {
            return Ok(secret.value.clone());
        }
        return Err(AuthStackError::store("secret has no encrypted value"));
    }
    let nonce_bytes = STANDARD
        .decode(secret.nonce_b64.trim())
        .map_err(|_| AuthStackError::store("secret nonce is corrupt"))?;
    let nonce: [u8; VAULT_NONCE_BYTES] = nonce_bytes
        .as_slice()
        .try_into()
        .map_err(|_| AuthStackError::store("secret nonce length is invalid"))?;
    let ciphertext = STANDARD
        .decode(secret.ciphertext_b64.trim())
        .map_err(|_| AuthStackError::store("secret ciphertext is corrupt"))?;
    let cipher = Aes256Gcm::new_from_slice(key.as_slice())
        .map_err(|_| AuthStackError::configuration("vault encryption key is invalid"))?;
    let aad = vault_secret_aad(org_id);
    let plain = cipher
        .decrypt(
            &Nonce::from(nonce),
            Payload {
                msg: &ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| AuthStackError::store("secret decryption failed"))?;
    String::from_utf8(plain).map_err(|_| AuthStackError::store("secret plaintext is not utf-8"))
}

pub(crate) fn secret_display_key(secret: &StoredSecret) -> String {
    if !secret.key.is_empty() {
        secret.key.clone()
    } else if !secret.name.is_empty() {
        secret.name.clone()
    } else {
        secret.id.clone()
    }
}

pub(crate) fn secret_display_label(secret: &StoredSecret) -> String {
    if !secret.label.is_empty() {
        secret.label.clone()
    } else if !secret.name.is_empty() && secret.name != secret.key {
        secret.name.clone()
    } else {
        secret_display_key(secret)
    }
}

pub(crate) fn secret_to_summary(secret: &StoredSecret) -> crate::contracts::SecretSummary {
    let key = secret_display_key(secret);
    let label = secret_display_label(secret);
    crate::contracts::SecretSummary {
        id: secret.id.clone(),
        key: key.clone(),
        label: label.clone(),
        name: if !key.is_empty() { key } else { label },
        description: secret.description.clone(),
        scope: if secret.scope.is_empty() {
            "user".to_owned()
        } else {
            secret.scope.clone()
        },
        created_at_ms: secret.created_at_ms,
        updated_at_ms: secret.updated_at_ms,
        masked_value: "••••••••".to_owned(),
    }
}

/// Load secrets, migrate legacy plaintext → AEAD ciphertext, and return records
/// with **in-memory** plaintext in `value` for connector resolution only.
///
/// `org_id` is the workspace vault owner (organization UUID).
pub(crate) async fn load_secrets_resolved(org_id: &str) -> AuthStackResult<Vec<StoredSecret>> {
    let mut secrets = load_secrets_raw(org_id).await?;
    if secrets.is_empty() {
        return Ok(secrets);
    }
    let (key_version, key_ring) = dashboard_vault_key_ring().await?;
    let key = key_ring
        .get(&key_version)
        .ok_or_else(|| AuthStackError::configuration("active vault key is unavailable"))?;
    let mut dirty = false;
    let now = dashboard_now_ms();
    for secret in &mut secrets {
        // Normalize legacy name-only rows.
        if secret.key.is_empty() && !secret.name.is_empty() {
            let candidate = secret
                .name
                .trim()
                .to_ascii_uppercase()
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect::<String>();
            if validate_vault_secret_key(&candidate).is_ok() {
                secret.key = candidate;
            } else {
                secret.key = format!("LEGACY_{}", &secret.id);
            }
            dirty = true;
        }
        if secret.label.is_empty() {
            secret.label = if !secret.name.is_empty() {
                secret.name.clone()
            } else {
                secret.key.clone()
            };
            dirty = true;
        }
        if secret.created_at_ms == 0 {
            secret.created_at_ms = now;
            dirty = true;
        }
        if secret.updated_at_ms == 0 {
            secret.updated_at_ms = secret.created_at_ms;
            dirty = true;
        }

        if !secret.value.is_empty() && secret.ciphertext_b64.is_empty() {
            let (nonce_b64, ciphertext_b64) = encrypt_vault_value(org_id, &secret.value, key)?;
            secret.nonce_b64 = nonce_b64;
            secret.ciphertext_b64 = ciphertext_b64;
            secret.key_version = key_version.clone();
            secret.value.clear();
            dirty = true;
        }
    }
    if dirty {
        // Persist ciphertext only (skip empty plaintext via serde skip).
        let to_save: Vec<StoredSecret> = secrets
            .iter()
            .map(|s| {
                let mut copy = s.clone();
                copy.value.clear();
                copy
            })
            .collect();
        save_secrets_raw(org_id, &to_save).await?;
    }
    // Decrypt into memory for execute path.
    for secret in &mut secrets {
        if secret.value.is_empty() && !secret.ciphertext_b64.is_empty() {
            // Try org AAD first; fall back to re-encrypt if decrypt fails (legacy user AAD).
            let stored_version = if secret.key_version.is_empty() {
                key_version.as_str()
            } else {
                secret.key_version.as_str()
            };
            let decrypt_key = key_ring.get(stored_version).ok_or_else(|| {
                AuthStackError::configuration(format!(
                    "vault key version {stored_version} is unavailable"
                ))
            })?;
            match decrypt_vault_value(org_id, secret, decrypt_key) {
                Ok(plain) => secret.value = plain,
                Err(_) if !secret.value.is_empty() => {}
                Err(_) => {
                    // Leave empty; create path always uses org AAD.
                    return Err(AuthStackError::store(
                        "secret decryption failed — re-create the secret in the org vault",
                    ));
                }
            }
        }
    }
    Ok(secrets)
}

pub async fn load_data_sources(
    user_id: &str,
) -> AuthStackResult<Vec<crate::contracts::DataSource>> {
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let store = profile_kv().await?;
        let Some(bytes) = store
            .get(dashboard_sources_key(user_id))
            .await
            .map_err(|e| AuthStackError::store(format!("sources read failed: {e}")))?
        else {
            return Ok(Vec::new());
        };
        Ok(serde_json::from_slice(&bytes).unwrap_or_default())
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = user_id;
        Ok(Vec::new())
    }
}

pub async fn save_data_sources(
    user_id: &str,
    sources: &[crate::contracts::DataSource],
) -> AuthStackResult<()> {
    if sources.len() > MAX_HTTP_SOURCES {
        return Err(AuthStackError::validation(format!(
            "at most {MAX_HTTP_SOURCES} HTTP sources"
        )));
    }
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let store = profile_kv().await?;
        let bytes = serde_json::to_vec(sources)
            .map_err(|e| AuthStackError::serialization(e.to_string()))?;
        store
            .set(dashboard_sources_key(user_id), bytes)
            .await
            .map_err(|e| AuthStackError::store(format!("sources write failed: {e}")))
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = (user_id, sources);
        Err(AuthStackError::configuration(
            "dashboard storage requires Spin key-value",
        ))
    }
}

pub(crate) async fn load_secrets_raw(org_id: &str) -> AuthStackResult<Vec<StoredSecret>> {
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let rows = execute_postgres(
            "SELECT secret_id, secret_key, label, description, scope, \
                    encode(ciphertext, 'base64') AS ciphertext_b64, \
                    encode(nonce, 'base64') AS nonce_b64, key_version, \
                    (EXTRACT(EPOCH FROM created_at) * 1000)::bigint AS created_at_ms, \
                    (EXTRACT(EPOCH FROM updated_at) * 1000)::bigint AS updated_at_ms \
             FROM fullstack_app.vault_secrets \
             WHERE organization_id = ?1::text::uuid ORDER BY secret_key, secret_id",
            vec![Value::String(org_id.to_owned())],
        )
        .await?;
        rows.iter()
            .map(|row| {
                Ok(StoredSecret {
                    id: required_string(row, "secret_id")?,
                    key: required_string(row, "secret_key")?,
                    label: required_string(row, "label")?,
                    name: String::new(),
                    description: required_string(row, "description")?,
                    scope: required_string(row, "scope")?,
                    value: String::new(),
                    ciphertext_b64: required_string(row, "ciphertext_b64")?.replace('\n', ""),
                    nonce_b64: required_string(row, "nonce_b64")?.replace('\n', ""),
                    mac_b64: String::new(),
                    key_version: required_string(row, "key_version")?,
                    created_at_ms: row_i64(row, "created_at_ms").unwrap_or_default().max(0) as u64,
                    updated_at_ms: row_i64(row, "updated_at_ms").unwrap_or_default().max(0) as u64,
                })
            })
            .collect()
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = org_id;
        Ok(Vec::new())
    }
}

pub(crate) async fn save_secrets_raw(
    org_id: &str,
    secrets: &[StoredSecret],
) -> AuthStackResult<()> {
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        if secrets.iter().any(|secret| !secret.value.is_empty()) {
            return Err(AuthStackError::store(
                "refusing to persist plaintext vault secret",
            ));
        }
        let payload = serde_json::to_value(secrets)
            .map_err(|error| AuthStackError::serialization(error.to_string()))?;
        execute_postgres(
            "WITH incoming AS ( \
                 SELECT item->>'id' AS secret_id, item->>'key' AS secret_key, \
                        item->>'label' AS label, item->>'description' AS description, \
                        item->>'scope' AS scope, decode(item->>'ciphertext_b64', 'base64') AS ciphertext, \
                        decode(item->>'nonce_b64', 'base64') AS nonce, item->>'key_version' AS key_version, \
                        COALESCE((item->>'created_at_ms')::bigint, 0) AS created_at_ms, \
                        COALESCE((item->>'updated_at_ms')::bigint, 0) AS updated_at_ms \
                 FROM jsonb_array_elements(?1::text::jsonb) AS item \
             ), deleted AS ( \
                 DELETE FROM fullstack_app.vault_secrets existing \
                 WHERE existing.organization_id = ?2::text::uuid \
                   AND NOT EXISTS (SELECT 1 FROM incoming WHERE incoming.secret_id = existing.secret_id) \
             ) \
             INSERT INTO fullstack_app.vault_secrets \
                 (organization_id, secret_id, secret_key, label, description, scope, ciphertext, nonce, key_version, created_at, updated_at) \
             SELECT ?2::text::uuid, secret_id, secret_key, label, description, scope, ciphertext, nonce, key_version, \
                    to_timestamp(created_at_ms / 1000.0), to_timestamp(updated_at_ms / 1000.0) \
             FROM incoming \
             ON CONFLICT (organization_id, secret_id) DO UPDATE SET \
                 secret_key = EXCLUDED.secret_key, label = EXCLUDED.label, \
                 description = EXCLUDED.description, scope = EXCLUDED.scope, \
                 ciphertext = EXCLUDED.ciphertext, nonce = EXCLUDED.nonce, \
                 key_version = EXCLUDED.key_version, \
                 revision = fullstack_app.vault_secrets.revision + 1, \
                 updated_at = EXCLUDED.updated_at",
            vec![payload, Value::String(org_id.to_owned())],
        )
        .await
        .map(|_| ())
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = (org_id, secrets);
        Err(AuthStackError::configuration(
            "dashboard storage requires Spin key-value",
        ))
    }
}

/// Detailed secrets migrate (used by admin/user migrate API).
pub async fn migrate_legacy_user_secrets_to_org_detailed(
    user_id: &str,
    org_id: &str,
    dry_run: bool,
) -> AuthStackResult<(bool, u32, u32, Vec<String>)> {
    // returns (copied, rows_copied, skipped_reenter, keys)
    let existing = load_secrets_raw(org_id).await?;
    if !existing.is_empty() {
        return Ok((false, 0, 0, Vec::new()));
    }
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let store = profile_kv().await?;
        let Some(bytes) = store
            .get(dashboard_secrets_legacy_user_key(user_id))
            .await
            .map_err(|e| AuthStackError::store(format!("legacy secrets read failed: {e}")))?
        else {
            return Ok((false, 0, 0, Vec::new()));
        };
        let legacy: Vec<StoredSecret> = serde_json::from_slice(&bytes).unwrap_or_default();
        if legacy.is_empty() {
            return Ok((false, 0, 0, Vec::new()));
        }
        let (key_version, key) = dashboard_vault_key_material().await?;
        let mut migrated = Vec::new();
        let mut reenter = Vec::new();
        for mut secret in legacy {
            if !secret.value.is_empty() {
                if !dry_run {
                    let (nonce_b64, ciphertext_b64) =
                        encrypt_vault_value(org_id, &secret.value, &key)?;
                    secret.nonce_b64 = nonce_b64;
                    secret.ciphertext_b64 = ciphertext_b64;
                    secret.key_version = key_version.clone();
                    secret.value.clear();
                }
                migrated.push(secret);
            } else if !secret.ciphertext_b64.is_empty() {
                reenter.push(secret_display_key(&secret));
            }
        }
        if migrated.is_empty() {
            return Ok((false, 0, reenter.len() as u32, reenter));
        }
        if !dry_run {
            save_secrets_raw(org_id, &migrated).await?;
            let _ = store
                .delete(dashboard_secrets_legacy_user_key(user_id))
                .await;
        }
        Ok((true, migrated.len() as u32, reenter.len() as u32, reenter))
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = (user_id, org_id, dry_run);
        Ok((false, 0, 0, Vec::new()))
    }
}

/// One-time best-effort: copy legacy user-scoped secrets into org vault when org vault empty.
pub async fn migrate_legacy_user_secrets_to_org(
    user_id: &str,
    org_id: &str,
) -> AuthStackResult<bool> {
    let (copied, _, _, _) =
        migrate_legacy_user_secrets_to_org_detailed(user_id, org_id, false).await?;
    Ok(copied)
}

pub async fn list_secret_summaries(
    org_id: &str,
) -> AuthStackResult<Vec<crate::contracts::SecretSummary>> {
    // Resolve migrates plaintext → ciphertext; summaries never include values.
    let secrets = load_secrets_resolved(org_id).await?;
    Ok(secrets.iter().map(secret_to_summary).collect())
}

pub async fn create_secret(
    org_id: &str,
    request: &crate::contracts::SecretCreateRequest,
) -> AuthStackResult<crate::contracts::SecretSummary> {
    let mut key = request.key.trim().to_owned();
    if key.is_empty() {
        key = request.name.trim().to_owned();
    }
    // Allow human names to be uppercased into env-like keys when possible.
    if validate_vault_secret_key(&key).is_err() {
        let candidate = key
            .trim()
            .to_ascii_uppercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect::<String>();
        key = candidate;
    }
    validate_vault_secret_key(&key)?;

    let label = {
        let l = request.label.trim();
        if l.is_empty() {
            let n = request.name.trim();
            if n.is_empty() {
                key.clone()
            } else {
                n.to_owned()
            }
        } else {
            l.to_owned()
        }
    };
    if label.len() > 120 {
        return Err(AuthStackError::validation("secret label is too long"));
    }
    let description = request.description.trim().to_owned();
    if description.len() > 500 {
        return Err(AuthStackError::validation("secret description is too long"));
    }
    let scope = match request.scope.trim().to_ascii_lowercase().as_str() {
        "" | "user" => "user".to_owned(),
        "app" => "app".to_owned(),
        _ => {
            return Err(AuthStackError::validation(
                "secret scope must be user or app",
            ));
        }
    };
    let value = request.value.as_str();
    if value.is_empty() || value.len() > MAX_VAULT_VALUE_BYTES {
        return Err(AuthStackError::validation(format!(
            "secret value must be 1–{MAX_VAULT_VALUE_BYTES} bytes"
        )));
    }

    // Migrate legacy plaintext rows first.
    let _ = load_secrets_resolved(org_id).await?;
    let mut secrets = load_secrets_raw(org_id).await?;

    if secrets.len() >= MAX_VAULT_SECRETS {
        return Err(AuthStackError::validation(format!(
            "at most {MAX_VAULT_SECRETS} vault secrets"
        )));
    }
    if secrets
        .iter()
        .any(|s| secret_display_key(s).eq_ignore_ascii_case(&key))
    {
        return Err(AuthStackError::validation(format!(
            "a secret with key {key} already exists"
        )));
    }

    let (key_version, vault_key) = dashboard_vault_key_material().await?;
    let (nonce_b64, ciphertext_b64) = encrypt_vault_value(org_id, value, &vault_key)?;
    let now = dashboard_now_ms();
    let id = new_id("sec");
    let stored = StoredSecret {
        id: id.clone(),
        key: key.clone(),
        label: label.clone(),
        name: key.clone(),
        description: description.clone(),
        scope: scope.clone(),
        value: String::new(),
        ciphertext_b64,
        nonce_b64,
        mac_b64: String::new(),
        key_version,
        created_at_ms: now,
        updated_at_ms: now,
    };
    secrets.push(stored.clone());
    save_secrets_raw(org_id, &secrets).await?;
    Ok(secret_to_summary(&stored))
}

pub async fn delete_secret(org_id: &str, secret_id: &str) -> AuthStackResult<()> {
    let mut secrets = load_secrets_raw(org_id).await?;
    let before = secrets.len();
    secrets.retain(|s| s.id != secret_id);
    if secrets.len() == before {
        return Err(AuthStackError::not_found("secret not found"));
    }
    save_secrets_raw(org_id, &secrets).await
}

pub async fn reveal_secret(
    org_id: &str,
    secret_id: &str,
) -> AuthStackResult<crate::contracts::SecretRevealResponse> {
    let secrets = load_secrets_resolved(org_id).await?;
    let secret = secrets
        .iter()
        .find(|s| s.id == secret_id)
        .ok_or_else(|| AuthStackError::not_found("secret not found"))?;
    let value = secret.value.clone();
    if value.is_empty() {
        return Err(AuthStackError::store("secret value is empty"));
    }
    tracing::info!(
        organization_id = %org_id,
        secret_id = %secret_id,
        secret_key = %secret_display_key(secret),
        "vault secret revealed"
    );
    Ok(crate::contracts::SecretRevealResponse {
        id: secret.id.clone(),
        key: secret_display_key(secret),
        value,
        reveal_ttl_seconds: VAULT_REVEAL_TTL_SECONDS,
    })
}
