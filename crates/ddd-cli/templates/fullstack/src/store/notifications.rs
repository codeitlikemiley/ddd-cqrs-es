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

pub async fn load_dashboard_notifications(
    user_id: &str,
) -> AuthStackResult<Vec<crate::contracts::DashboardNotification>> {
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let store = profile_kv().await?;
        let key = dashboard_notifs_key(user_id);
        let Some(bytes) = store
            .get(&key)
            .await
            .map_err(|error| AuthStackError::store(format!("notifications read failed: {error}")))?
        else {
            let notifs = default_notifications();
            save_dashboard_notifications(user_id, &notifs).await?;
            return Ok(notifs);
        };
        match serde_json::from_slice::<Vec<crate::contracts::DashboardNotification>>(&bytes) {
            Ok(list) => Ok(list),
            _ => {
                let notifs = default_notifications();
                save_dashboard_notifications(user_id, &notifs).await?;
                Ok(notifs)
            }
        }
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = user_id;
        Ok(default_notifications())
    }
}

pub async fn save_dashboard_notifications(
    user_id: &str,
    notifications: &[crate::contracts::DashboardNotification],
) -> AuthStackResult<()> {
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let store = profile_kv().await?;
        let bytes = serde_json::to_vec(notifications)
            .map_err(|error| AuthStackError::serialization(error.to_string()))?;
        store
            .set(dashboard_notifs_key(user_id), bytes)
            .await
            .map_err(|error| AuthStackError::store(format!("notifications write failed: {error}")))
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = (user_id, notifications);
        Err(AuthStackError::configuration(
            "dashboard storage requires Spin key-value",
        ))
    }
}

pub async fn dismiss_dashboard_notification(
    user_id: &str,
    notification_id: &str,
) -> AuthStackResult<Vec<crate::contracts::DashboardNotification>> {
    let mut list = load_dashboard_notifications(user_id).await?;
    list.retain(|item| item.id != notification_id);
    save_dashboard_notifications(user_id, &list).await?;
    Ok(list)
}

