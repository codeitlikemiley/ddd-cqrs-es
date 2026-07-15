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
    org_id: &str,
) -> AuthStackResult<Vec<crate::contracts::DashboardNotification>> {
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let rows = execute_postgres(
            "SELECT payload FROM fullstack_app.dashboard_notifications \
             WHERE organization_id = ?1::text::uuid ORDER BY created_at, notification_id",
            vec![Value::String(org_id.to_owned())],
        )
        .await?;
        if rows.is_empty() {
            let notifs = default_notifications();
            save_dashboard_notifications(org_id, &notifs).await?;
            return Ok(notifs);
        }
        rows.iter()
            .map(|row| {
                serde_json::from_str(&required_string(row, "payload")?)
                    .map_err(|error| AuthStackError::serialization(error.to_string()))
            })
            .collect()
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = org_id;
        Ok(default_notifications())
    }
}

pub async fn save_dashboard_notifications(
    org_id: &str,
    notifications: &[crate::contracts::DashboardNotification],
) -> AuthStackResult<()> {
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let payload = serde_json::to_value(notifications)
            .map_err(|error| AuthStackError::serialization(error.to_string()))?;
        execute_postgres(
            "WITH incoming AS ( \
                 SELECT item->>'id' AS notification_id, item AS payload \
                 FROM jsonb_array_elements(?1::text::jsonb) AS item \
             ), deleted AS ( \
                 DELETE FROM fullstack_app.dashboard_notifications existing \
                 WHERE existing.organization_id = ?2::text::uuid \
                   AND NOT EXISTS (SELECT 1 FROM incoming WHERE incoming.notification_id = existing.notification_id) \
             ) \
             INSERT INTO fullstack_app.dashboard_notifications \
                 (organization_id, notification_id, payload) \
             SELECT ?2::text::uuid, notification_id, payload FROM incoming \
             ON CONFLICT (organization_id, notification_id) DO UPDATE SET payload = EXCLUDED.payload",
            vec![payload, Value::String(org_id.to_owned())],
        )
        .await
        .map(|_| ())
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = (org_id, notifications);
        Err(AuthStackError::configuration(
            "dashboard storage requires Spin key-value",
        ))
    }
}

pub async fn dismiss_dashboard_notification(
    org_id: &str,
    notification_id: &str,
) -> AuthStackResult<Vec<crate::contracts::DashboardNotification>> {
    let mut list = load_dashboard_notifications(org_id).await?;
    list.retain(|item| item.id != notification_id);
    save_dashboard_notifications(org_id, &list).await?;
    Ok(list)
}
