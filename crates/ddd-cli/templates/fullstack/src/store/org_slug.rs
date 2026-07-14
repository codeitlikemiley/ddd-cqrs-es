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

/// Reserved workspace URL segments (not usable as org slugs).
pub(crate) const RESERVED_ORG_SLUGS: &[&str] = &[
    "new",
    "settings",
    "admin",
    "api",
    "account",
    "org",
    "orgs",
    "organizations",
    "onboarding",
    "dashboard",
    "login",
    "register",
    "auth",
    "u",
    "invitations",
    "vault",
    "www",
    "app",
    "static",
    "assets",
];

/// Suggest a URL slug from a display name.
pub fn suggest_org_slug(name: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in name.trim().chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_owned();
    if trimmed.is_empty() {
        "workspace".to_owned()
    } else if trimmed.len() > 48 {
        trimmed.chars().take(48).collect::<String>().trim_end_matches('-').to_owned()
    } else {
        trimmed
    }
}

/// Validate org URL slug: `^[a-z][a-z0-9-]{1,47}$`.
pub fn validate_org_slug(slug: &str) -> AuthStackResult<()> {
    let slug = slug.trim();
    if slug.len() < 2 || slug.len() > 48 {
        return Err(AuthStackError::validation(
            "workspace URL must be 2–48 characters",
        ));
    }
    let mut chars = slug.chars();
    let Some(first) = chars.next() else {
        return Err(AuthStackError::validation("workspace URL is required"));
    };
    if !first.is_ascii_lowercase() {
        return Err(AuthStackError::validation(
            "workspace URL must start with a lowercase letter",
        ));
    }
    if !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err(AuthStackError::validation(
            "workspace URL may only contain a–z, 0–9, and hyphens",
        ));
    }
    if slug.starts_with('-') || slug.ends_with('-') || slug.contains("--") {
        return Err(AuthStackError::validation(
            "workspace URL cannot start/end with a hyphen or contain double hyphens",
        ));
    }
    if RESERVED_ORG_SLUGS.contains(&slug) {
        return Err(AuthStackError::validation(format!(
            "workspace URL “{slug}” is reserved"
        )));
    }
    Ok(())
}

pub async fn resolve_org_id_for_slug(slug: &str) -> AuthStackResult<String> {
    validate_org_slug(slug)?;
    // Prefer Postgres (via org list cache is done in application); KV is fallback.
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        // Direct SQL via app DATABASE_URL when available would be ideal; for now
        // KV remains the resolve path and is dual-written on create/list.
        let store = profile_kv().await?;
        if let Some(bytes) = store
            .get(org_slug_key(slug))
            .await
            .map_err(|e| AuthStackError::store(format!("org slug read failed: {e}")))?
        {
            let id = String::from_utf8(bytes)
                .map_err(|_| AuthStackError::store("org slug mapping is corrupt"))?;
            if !id.trim().is_empty() {
                return Ok(id);
            }
        }
        // Fallback: scan is not available; try postgres lookup.
        if let Ok(id) = resolve_org_id_for_slug_postgres(slug).await {
            let _ = register_org_slug(&id, slug).await;
            return Ok(id);
        }
        Err(AuthStackError::not_found("workspace not found"))
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = slug;
        Err(AuthStackError::configuration(
            "org slug storage requires Spin key-value",
        ))
    }
}

#[cfg(all(feature = "postgres", runtime_spin))]
pub(crate) async fn resolve_org_id_for_slug_postgres(slug: &str) -> AuthStackResult<String> {
    let rows = execute_sql(
        "SELECT organization_id::text AS organization_id \
         FROM auth_organizations \
         WHERE slug = $1 AND status = 'active' \
         LIMIT 1",
        vec![serde_json::Value::String(slug.to_owned())],
    )
    .await?;
    rows.first()
        .and_then(|row| {
            row.get("organization_id")
                .and_then(|v| v.as_str())
                .map(str::to_owned)
        })
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AuthStackError::not_found("workspace not found"))
}

pub async fn slug_for_organization(org_id: &str) -> AuthStackResult<Option<String>> {
    let org_id = org_id.trim();
    if org_id.is_empty() {
        return Ok(None);
    }
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let store = profile_kv().await?;
        let Some(bytes) = store
            .get(org_id_slug_key(org_id))
            .await
            .map_err(|e| AuthStackError::store(format!("org id slug read failed: {e}")))?
        else {
            return Ok(None);
        };
        Ok(String::from_utf8(bytes).ok().filter(|s| !s.trim().is_empty()))
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = org_id;
        Ok(None)
    }
}

/// Register a unique slug for an organization. Fails if slug is taken by another org.
pub async fn register_org_slug(org_id: &str, slug: &str) -> AuthStackResult<()> {
    validate_org_slug(slug)?;
    let org_id = org_id.trim();
    if org_id.is_empty() {
        return Err(AuthStackError::validation("organization_id is required"));
    }
    let slug = slug.trim().to_ascii_lowercase();
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let store = profile_kv().await?;
        if let Some(existing) = store
            .get(org_slug_key(&slug))
            .await
            .map_err(|e| AuthStackError::store(format!("org slug read failed: {e}")))?
        {
            let existing_id = String::from_utf8(existing).unwrap_or_default();
            if existing_id != org_id {
                return Err(AuthStackError::validation(format!(
                    "workspace URL “{slug}” is already taken"
                )));
            }
            // Same org re-registering — ok.
            return Ok(());
        }
        // Clear previous slug for this org if any.
        if let Some(prev) = store
            .get(org_id_slug_key(org_id))
            .await
            .map_err(|e| AuthStackError::store(format!("org id slug read failed: {e}")))?
        {
            if let Ok(prev_slug) = String::from_utf8(prev) {
                let _ = store.delete(org_slug_key(&prev_slug)).await;
            }
        }
        store
            .set(org_slug_key(&slug), org_id.as_bytes())
            .await
            .map_err(|e| AuthStackError::store(format!("org slug write failed: {e}")))?;
        store
            .set(org_id_slug_key(org_id), slug.as_bytes())
            .await
            .map_err(|e| AuthStackError::store(format!("org id slug write failed: {e}")))?;
        Ok(())
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = (org_id, slug);
        Err(AuthStackError::configuration(
            "org slug storage requires Spin key-value",
        ))
    }
}

/// Ensure each org has a slug; auto-register from name when missing (best-effort unique).
pub async fn ensure_org_slug(org_id: &str, name: &str) -> AuthStackResult<String> {
    if let Some(existing) = slug_for_organization(org_id).await? {
        return Ok(existing);
    }
    let base = suggest_org_slug(name);
    let mut candidate = base.clone();
    for attempt in 0..32u32 {
        if attempt > 0 {
            candidate = format!("{base}-{}", attempt + 1);
            if candidate.len() > 48 {
                candidate = format!("ws-{}", &org_id.chars().take(8).collect::<String>());
            }
        }
        if validate_org_slug(&candidate).is_err() {
            continue;
        }
        if resolve_org_id_for_slug(&candidate).await.is_ok() {
            continue;
        }
        match register_org_slug(org_id, &candidate).await {
            Ok(()) => return Ok(candidate),
            Err(_) => continue,
        }
    }
    Err(AuthStackError::validation(
        "could not allocate a unique workspace URL",
    ))
}

