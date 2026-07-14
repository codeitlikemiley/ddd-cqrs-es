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

pub async fn storage_status() -> AuthStackResult<StorageStatusResponse> {
    initialize_schema_async().await?;
    let summary_rows = execute_sql(
        "SELECT COUNT(*) AS event_count, COALESCE(MAX(sequence), 0) AS latest_sequence FROM auth_audit_log",
        Vec::new(),
    )
    .await?;
    let summary = summary_rows.first();
    let event_count = summary
        .and_then(|row| row_i64(row, "event_count"))
        .unwrap_or_default()
        .max(0) as u64;
    let latest_sequence = summary
        .and_then(|row| row_i64(row, "latest_sequence"))
        .unwrap_or_default()
        .max(0) as u64;
    let event_types = execute_sql(
        "SELECT action AS event_type, COUNT(*) AS count \
         FROM auth_audit_log GROUP BY action ORDER BY action",
        Vec::new(),
    )
    .await?
    .into_iter()
    .map(|row| {
        Ok(StorageEventTypeCount {
            event_type: required_string(&row, "event_type")?,
            count: row_i64(&row, "count").unwrap_or_default().max(0) as u64,
        })
    })
    .collect::<AuthStackResult<Vec<_>>>()?;
    Ok(StorageStatusResponse {
        event_count,
        latest_sequence,
        event_types,
        checkpoints: Vec::new(),
    })
}

pub async fn catch_up_storage_projections(
    batch_limit: Option<usize>,
) -> AuthStackResult<Vec<StorageProjectionRunResponse>> {
    initialize_schema_async().await?;
    if batch_limit.is_some_and(|limit| limit == 0 || limit > 10_000) {
        return Err(AuthStackError::validation(
            "projection batch limit is invalid",
        ));
    }
    Ok(Vec::new())
}

pub(crate) fn required_string(row: &Value, key: &str) -> AuthStackResult<String> {
    row_string(row, key).ok_or_else(|| AuthStackError::store(format!("missing column '{key}'")))
}

pub(crate) fn row_string(row: &Value, key: &str) -> Option<String> {
    row.get(key).and_then(|value| match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Array(_) | Value::Object(_) => serde_json::to_string(value).ok(),
        _ => None,
    })
}

pub(crate) fn row_i64(row: &Value, key: &str) -> Option<i64> {
    row.get(key).and_then(Value::as_i64)
}

#[cfg(feature = "mail-capture")]
pub(crate) fn random_bytes(len: usize) -> AuthStackResult<Vec<u8>> {
    #[cfg(all(target_arch = "wasm32", feature = "ssr"))]
    {
        Ok(wasip3::random::random::get_random_bytes(len as u64))
    }

    #[cfg(not(all(target_arch = "wasm32", feature = "ssr")))]
    {
        let mut bytes = vec![0_u8; len];
        getrandom::getrandom(&mut bytes).map_err(|error| {
            AuthStackError::store(format!("secure randomness unavailable: {error}"))
        })?;
        Ok(bytes)
    }
}

pub(crate) async fn mail_transport() -> String {
    store_config_value("AUTH_MAIL_TRANSPORT")
        .await
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "capture".to_string())
        .trim()
        .to_ascii_lowercase()
}

/// Validates security-sensitive runtime variables without touching storage.
///
/// Schema installation and checksum verification are separate deployment and
/// health gates so the trusted-ingress hot path never performs migration I/O.
pub async fn validate_runtime_security_config() -> AuthStackResult<()> {
    if RUNTIME_SECURITY_VALIDATED.load(Ordering::Acquire) {
        return Ok(());
    }
    let lock = RUNTIME_SECURITY_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock.lock().await;
    if RUNTIME_SECURITY_VALIDATED.load(Ordering::Acquire) {
        return Ok(());
    }
    validate_runtime_security_config_uncached().await?;
    RUNTIME_SECURITY_VALIDATED.store(true, Ordering::Release);
    Ok(())
}

pub(crate) async fn validate_runtime_security_config_uncached() -> AuthStackResult<()> {
    validate_spicedb_runtime_config().await?;
    if !config_bool(AUTH_PRODUCTION_MODE, false).await {
        return Ok(());
    }
    if !config_bool("AUTH_COOKIE_SECURE", false).await {
        return Err(AuthStackError::configuration(
            "production requires AUTH_COOKIE_SECURE=true",
        ));
    }
    if config_bool("AUTH_DEV_TOOLS", false).await {
        return Err(AuthStackError::configuration(
            "production forbids AUTH_DEV_TOOLS",
        ));
    }
    if !config_bool("AUTH_REQUIRE_TRUSTED_INGRESS", false).await {
        return Err(AuthStackError::configuration(
            "production requires AUTH_REQUIRE_TRUSTED_INGRESS=true",
        ));
    }
    let ingress_key = store_config_value("AUTH_TRUSTED_INGRESS_KEY_BASE64")
        .await
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            AuthStackError::configuration("production requires AUTH_TRUSTED_INGRESS_KEY_BASE64")
        })?;
    let ingress_key = STANDARD
        .decode(ingress_key.trim())
        .ok()
        .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
        .ok_or_else(|| {
            AuthStackError::configuration("AUTH_TRUSTED_INGRESS_KEY_BASE64 must decode to 32 bytes")
        })?;
    if store_config_value("AUTH_TRUSTED_INGRESS_AUDIENCE")
        .await
        .is_none_or(|value| value.trim().is_empty() || value.len() > 256)
    {
        return Err(AuthStackError::configuration(
            "production requires a bounded AUTH_TRUSTED_INGRESS_AUDIENCE",
        ));
    }
    let ingress_max_age = store_config_value("AUTH_TRUSTED_INGRESS_MAX_AGE_SECONDS")
        .await
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(5);
    if !(1..=30).contains(&ingress_max_age) {
        return Err(AuthStackError::configuration(
            "AUTH_TRUSTED_INGRESS_MAX_AGE_SECONDS must be between 1 and 30",
        ));
    }
    let public_base_url = store_config_value("AUTH_PUBLIC_BASE_URL")
        .await
        .unwrap_or_default();
    if !public_base_url.starts_with("https://") {
        return Err(AuthStackError::configuration(
            "production requires an https AUTH_PUBLIC_BASE_URL",
        ));
    }
    if store_config_value("AUTH_CSRF_SECRET")
        .await
        .is_none_or(|value| value.trim().len() < 32)
    {
        return Err(AuthStackError::configuration(
            "production requires AUTH_CSRF_SECRET with at least 32 characters",
        ));
    }
    let vault_key: [u8; 32] = store_config_value(MFA_VAULT_KEY)
        .await
        .and_then(|value| STANDARD.decode(value.trim()).ok())
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| {
            AuthStackError::configuration(
                "production AUTH_VAULT_KEY_BASE64 must decode to 32 bytes",
            )
        })?;
    if store_config_value("AUTH_VAULT_KEY_VERSION")
        .await
        .is_none_or(|value| {
            value.trim().is_empty()
                || value == "development-v1"
                || value.len() > 128
                || value.chars().any(char::is_control)
        })
    {
        return Err(AuthStackError::configuration(
            "production requires a bounded non-development AUTH_VAULT_KEY_VERSION",
        ));
    }
    let recovery_pepper = store_config_value(MFA_RECOVERY_PEPPER)
        .await
        .and_then(|value| STANDARD.decode(value.trim()).ok())
        .filter(|bytes| (16..=1_024).contains(&bytes.len()))
        .ok_or_else(|| {
            AuthStackError::configuration(
                "production AUTH_RECOVERY_CODE_PEPPER_BASE64 must decode to 16-1024 bytes",
            )
        })?;
    let outbox_key: [u8; 32] = store_config_value("AUTH_OUTBOX_KEY_BASE64")
        .await
        .and_then(|value| STANDARD.decode(value.trim()).ok())
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| {
            AuthStackError::configuration(
                "production AUTH_OUTBOX_KEY_BASE64 must decode to 32 bytes",
            )
        })?;
    let development_outbox_key: [u8; 32] =
        Sha256::digest(b"fullstack-development-outbox-key").into();
    if vault_key == development_outbox_key || outbox_key == development_outbox_key {
        return Err(AuthStackError::configuration(
            "production forbids development encryption keys",
        ));
    }
    if store_config_value("AUTH_OUTBOX_KEY_VERSION")
        .await
        .is_none_or(|value| value.trim().is_empty() || value == "development-v1")
    {
        return Err(AuthStackError::configuration(
            "production requires a non-development AUTH_OUTBOX_KEY_VERSION",
        ));
    }
    if ingress_key == vault_key
        || ingress_key == outbox_key
        || vault_key == outbox_key
        || recovery_pepper.as_slice() == ingress_key.as_slice()
        || recovery_pepper.as_slice() == vault_key.as_slice()
        || recovery_pepper.as_slice() == outbox_key.as_slice()
    {
        return Err(AuthStackError::configuration(
            "production requires distinct ingress, vault, outbox, and recovery secrets",
        ));
    }
    match mail_transport().await.as_str() {
        "http" => {}
        "capture" => {
            return Err(AuthStackError::configuration(
                "production forbids capture mail; run the native outbox worker with HTTP mail",
            ));
        }
        _ => {
            return Err(AuthStackError::configuration(
                "production AUTH_MAIL_TRANSPORT must be http",
            ));
        }
    }
    Ok(())
}

pub(crate) async fn validate_spicedb_runtime_config() -> AuthStackResult<()> {
    if !config_bool("AUTH_SPICEDB_ENABLED", false).await {
        return Ok(());
    }
    #[cfg(all(feature = "spicedb", runtime_spin))]
    {
        let check_endpoint = store_config_value("AUTH_SPICEDB_CHECK_URL")
            .await
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| AuthStackError::configuration("AUTH_SPICEDB_CHECK_URL is required"))?;
        SpiceDbEndpoint::new(check_endpoint.trim())
            .map_err(|_| AuthStackError::configuration("AUTH_SPICEDB_CHECK_URL is invalid"))?;
        require_production_secret("AUTH_SPICEDB_CHECK_TOKEN").await?;
        Ok(())
    }
    #[cfg(not(all(feature = "spicedb", runtime_spin)))]
    {
        Err(AuthStackError::configuration(
            "AUTH_SPICEDB_ENABLED requires the spicedb feature on Spin",
        ))
    }
}

#[cfg(all(feature = "spicedb", runtime_spin))]
pub(crate) async fn require_production_secret(name: &str) -> AuthStackResult<()> {
    if store_config_value(name)
        .await
        .is_some_and(|value| !value.trim().is_empty())
    {
        Ok(())
    } else {
        Err(AuthStackError::configuration(format!(
            "production requires {name}"
        )))
    }
}

pub(crate) async fn config_bool(name: &str, default: bool) -> bool {
    store_config_value(name)
        .await
        .map(|value| truthy(&value))
        .unwrap_or(default)
}

pub(crate) async fn store_config_value(name: &str) -> Option<String> {
    #[cfg(all(runtime_spin, not(test)))]
    {
        let variable_name = name.to_ascii_lowercase();
        if let Ok(value) = spin_sdk::variables::get(&variable_name).await {
            return Some(value);
        }
    }

    std::env::var(name).ok()
}

pub(crate) fn truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on" | "enabled"
    )
}

#[cfg(feature = "mail-capture")]
pub(crate) fn secure_storage_id(kind: &str) -> AuthStackResult<String> {
    Ok(format!(
        "{kind}_{}",
        URL_SAFE_NO_PAD.encode(random_bytes(32)?)
    ))
}

#[cfg(feature = "mail-capture")]
pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

pub async fn health_status() -> AuthStackResult<HealthStatusResponse> {
    initialize_schema_async().await?;
    Ok(HealthStatusResponse {
        status: "ok".to_owned(),
        storage_backend: match storage_backend().await? {
            StorageBackend::Postgres => "postgres",
        }
        .to_owned(),
        mail_transport: mail_transport().await,
        authorization_provider: "embedded-cedar".to_owned(),
        production_mode: config_bool(AUTH_PRODUCTION_MODE, false).await,
    })
}

