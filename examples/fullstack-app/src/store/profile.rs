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

pub(crate) const PROFILE_USER_PREFIX: &str = "app_profile:user:";
pub(crate) const PROFILE_HANDLE_PREFIX: &str = "app_profile:handle:";
pub(crate) const MAX_AVATAR_DATA_URL_BYTES: usize = 350_000;
pub(crate) const MAX_NAME_LEN: usize = 60;
pub(crate) const MAX_DISPLAY_NAME_LEN: usize = 80;
pub(crate) const USERNAME_MIN: usize = 3;
pub(crate) const USERNAME_MAX: usize = 30;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub(crate) struct StoredProfile {
    user_id: String,
    first_name: String,
    last_name: String,
    display_name: String,
    username: String,
    is_public: bool,
    #[serde(default)]
    avatar_data_url: Option<String>,
}

impl StoredProfile {
    fn empty(user_id: impl Into<String>) -> Self {
        Self {
            user_id: user_id.into(),
            first_name: String::new(),
            last_name: String::new(),
            display_name: String::new(),
            username: String::new(),
            is_public: false,
            avatar_data_url: None,
        }
    }

    fn to_view(&self, email: Option<String>) -> crate::contracts::ProfileView {
        let public_path = if self.username.is_empty() {
            None
        } else {
            Some(format!("/u/{}", self.username))
        };
        crate::contracts::ProfileView {
            email,
            first_name: self.first_name.clone(),
            last_name: self.last_name.clone(),
            display_name: self.display_name.clone(),
            username: self.username.clone(),
            is_public: self.is_public,
            avatar_data_url: self.avatar_data_url.clone(),
            public_path,
        }
    }

    fn to_public_view(&self) -> crate::contracts::PublicProfileView {
        crate::contracts::PublicProfileView {
            username: self.username.clone(),
            display_name: self.display_name.clone(),
            first_name: self.first_name.clone(),
            last_name: self.last_name.clone(),
            avatar_data_url: self.avatar_data_url.clone(),
        }
    }
}

#[cfg(all(feature = "postgres", runtime_spin))]
pub(crate) async fn profile_kv() -> AuthStackResult<spin_sdk::key_value::Store> {
    spin_sdk::key_value::Store::open_default()
        .await
        .map_err(|error| AuthStackError::store(format!("profile store unavailable: {error}")))
}

pub(crate) fn profile_user_key(user_id: &str) -> String {
    format!("{PROFILE_USER_PREFIX}{user_id}")
}

pub(crate) fn profile_handle_key(username: &str) -> String {
    format!("{PROFILE_HANDLE_PREFIX}{}", username.to_ascii_lowercase())
}

pub(crate) fn normalize_name(label: &str, value: &str, max: usize) -> AuthStackResult<String> {
    let trimmed = value.trim();
    if trimmed.chars().count() > max {
        return Err(AuthStackError::validation(format!(
            "{label} must be at most {max} characters"
        )));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(AuthStackError::validation(format!(
            "{label} contains invalid characters"
        )));
    }
    Ok(trimmed.to_owned())
}

pub(crate) fn normalize_username(value: &str) -> AuthStackResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    let lower = trimmed.to_ascii_lowercase();
    let len = lower.chars().count();
    if len < USERNAME_MIN || len > USERNAME_MAX {
        return Err(AuthStackError::validation(format!(
            "username must be {USERNAME_MIN}–{USERNAME_MAX} characters"
        )));
    }
    if !lower
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(AuthStackError::validation(
            "username may only use letters, numbers, and underscores",
        ));
    }
    if lower.starts_with('_') || lower.ends_with('_') {
        return Err(AuthStackError::validation(
            "username cannot start or end with an underscore",
        ));
    }
    const RESERVED: &[&str] = &[
        "admin",
        "api",
        "account",
        "auth",
        "dashboard",
        "login",
        "logout",
        "register",
        "settings",
        "support",
        "system",
        "me",
        "null",
        "undefined",
        "u",
        "www",
        "root",
        "help",
        "about",
    ];
    if RESERVED.contains(&lower.as_str()) {
        return Err(AuthStackError::validation("that username is reserved"));
    }
    Ok(lower)
}

pub(crate) fn validate_avatar_data_url(value: &str) -> AuthStackResult<Option<String>> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.len() > MAX_AVATAR_DATA_URL_BYTES {
        return Err(AuthStackError::validation(
            "avatar is too large (use an image under ~250 KB)",
        ));
    }
    let ok = trimmed.starts_with("data:image/png;base64,")
        || trimmed.starts_with("data:image/jpeg;base64,")
        || trimmed.starts_with("data:image/jpg;base64,")
        || trimmed.starts_with("data:image/webp;base64,")
        || trimmed.starts_with("data:image/gif;base64,");
    if !ok {
        return Err(AuthStackError::validation(
            "avatar must be a PNG, JPEG, WebP, or GIF data URL",
        ));
    }
    Ok(Some(trimmed.to_owned()))
}

#[cfg(all(feature = "postgres", runtime_spin))]
pub(crate) async fn load_stored_profile(user_id: &str) -> AuthStackResult<StoredProfile> {
    let store = profile_kv().await?;
    let key = profile_user_key(user_id);
    let Some(bytes) = store
        .get(&key)
        .await
        .map_err(|error| AuthStackError::store(format!("profile read failed: {error}")))?
    else {
        return Ok(StoredProfile::empty(user_id));
    };
    serde_json::from_slice(&bytes)
        .map_err(|error| AuthStackError::store(format!("profile payload is corrupt: {error}")))
}

#[cfg(all(feature = "postgres", runtime_spin))]
pub(crate) async fn save_stored_profile(profile: &StoredProfile) -> AuthStackResult<()> {
    let store = profile_kv().await?;
    let bytes = serde_json::to_vec(profile)
        .map_err(|error| AuthStackError::serialization(error.to_string()))?;
    store
        .set(profile_user_key(&profile.user_id), bytes)
        .await
        .map_err(|error| AuthStackError::store(format!("profile write failed: {error}")))
}

pub async fn get_profile_for_user(
    user_id: &str,
    email: Option<String>,
) -> AuthStackResult<crate::contracts::ProfileView> {
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        Ok(load_stored_profile(user_id).await?.to_view(email))
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = user_id;
        Ok(StoredProfile::empty("").to_view(email))
    }
}

pub async fn update_profile_for_user(
    user_id: &str,
    email: Option<String>,
    request: crate::contracts::ProfileUpdateRequest,
) -> AuthStackResult<crate::contracts::ProfileView> {
    let first_name = normalize_name("first name", &request.first_name, MAX_NAME_LEN)?;
    let last_name = normalize_name("last name", &request.last_name, MAX_NAME_LEN)?;
    let display_name = normalize_name("display name", &request.display_name, MAX_DISPLAY_NAME_LEN)?;
    let username = normalize_username(&request.username)?;

    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = (
            user_id,
            email,
            first_name,
            last_name,
            display_name,
            username,
            request,
        );
        return Err(AuthStackError::configuration(
            "profile storage requires Spin key-value",
        ));
    }

    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let mut profile = load_stored_profile(user_id).await?;
        let previous_username = profile.username.clone();

        if let Some(avatar) = request.avatar_data_url.as_ref() {
            profile.avatar_data_url = validate_avatar_data_url(avatar)?;
        }

        // Username uniqueness (handle index).
        if !username.is_empty() && username != previous_username {
            let store = profile_kv().await?;
            let handle_key = profile_handle_key(&username);
            if let Some(existing) = store
                .get(&handle_key)
                .await
                .map_err(|error| AuthStackError::store(format!("handle lookup failed: {error}")))?
            {
                let owner = String::from_utf8_lossy(&existing);
                if owner != user_id {
                    return Err(AuthStackError::conflict("that username is already taken"));
                }
            }
        }

        profile.first_name = first_name;
        profile.last_name = last_name;
        profile.display_name = display_name;
        profile.username = username.clone();
        profile.is_public = request.is_public;

        let store = profile_kv().await?;
        if !previous_username.is_empty() && previous_username != username {
            let _ = store.delete(profile_handle_key(&previous_username)).await;
        }
        if !username.is_empty() {
            store
                .set(profile_handle_key(&username), user_id.as_bytes())
                .await
                .map_err(|error| AuthStackError::store(format!("handle write failed: {error}")))?;
        }

        save_stored_profile(&profile).await?;
        Ok(profile.to_view(email))
    }
}

pub async fn get_public_profile_by_username(
    username: &str,
) -> AuthStackResult<crate::contracts::PublicProfileView> {
    let username = normalize_username(username)?;
    if username.is_empty() {
        return Err(AuthStackError::not_found("profile not found"));
    }

    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = username;
        return Err(AuthStackError::not_found("profile not found"));
    }

    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let store = profile_kv().await?;
        let handle_key = profile_handle_key(&username);
        let Some(user_bytes) = store
            .get(&handle_key)
            .await
            .map_err(|error| AuthStackError::store(format!("handle lookup failed: {error}")))?
        else {
            return Err(AuthStackError::not_found("profile not found"));
        };
        let user_id = String::from_utf8_lossy(&user_bytes).into_owned();
        let profile = load_stored_profile(&user_id).await?;
        if !profile.is_public || profile.username != username {
            return Err(AuthStackError::not_found("profile not found"));
        }
        Ok(profile.to_public_view())
    }
}

// ── Dashboard board (Spin KV) ─────────────────────────────────────────────
// Layout / resources / queries / secrets are **workspace (org) scoped**.
// Notifications + legacy HTTP sources remain per-user.
