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

pub(crate) fn new_id(prefix: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("{prefix}{ms:x}")
}

pub fn default_dashboard_layout_public() -> crate::contracts::DashboardLayout {
    default_dashboard_layout()
}

pub(crate) fn default_dashboard_layout() -> crate::contracts::DashboardLayout {
    use crate::contracts::{
        BoardContainerKind, BoardNode, DashboardLayout, DashboardWidgetKind, HttpDisplayMode,
        WidgetBind,
    };
    let widget = |index: usize, kind: DashboardWidgetKind| BoardNode::Widget {
        id: format!("w{index}"),
        kind: kind.clone(),
        col_span: kind.default_span(),
        note_text: if matches!(kind, DashboardWidgetKind::Notes) {
            Some(String::new())
        } else {
            None
        },
        source_id: None,
        bind: WidgetBind::default(),
        http_mode: HttpDisplayMode::List,
    };
    // Metrics row in a container; remaining tiles at root.
    let metrics = BoardNode::Container {
        id: "c-metrics".to_owned(),
        kind: BoardContainerKind::Row,
        col_span: 12,
        children: vec![
            widget(0, DashboardWidgetKind::MetricSession),
            widget(1, DashboardWidgetKind::MetricDevices),
            widget(2, DashboardWidgetKind::MetricOrgs),
            widget(3, DashboardWidgetKind::MetricSecurity),
        ],
    };
    let activity_row = BoardNode::Container {
        id: "c-main".to_owned(),
        kind: BoardContainerKind::Row,
        col_span: 12,
        children: vec![
            widget(4, DashboardWidgetKind::Activity),
            widget(5, DashboardWidgetKind::Notifications),
        ],
    };
    DashboardLayout {
        version: 2,
        nodes: vec![
            metrics,
            activity_row,
            widget(6, DashboardWidgetKind::Sessions),
            widget(7, DashboardWidgetKind::Organizations),
            widget(8, DashboardWidgetKind::SecurityPosture),
            widget(9, DashboardWidgetKind::Checklist),
        ],
        widgets: Vec::new(),
    }
}

pub use crate::app::dashboard::bind::json_path_get;

/// SSRF: reject private / link-local / metadata hosts unless allow_private.
pub fn validate_http_url(url: &str, allow_private: bool) -> AuthStackResult<()> {
    let url = url.trim();
    if url.is_empty() {
        return Err(AuthStackError::validation("url is required"));
    }
    if url.len() > 2_048 {
        return Err(AuthStackError::validation("url is too long"));
    }
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return Err(AuthStackError::validation("url must be http(s)"));
    }
    let without_scheme = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
        .unwrap_or("");
    let host_port = without_scheme
        .split('/')
        .next()
        .unwrap_or("")
        .split('@')
        .next_back()
        .unwrap_or("");
    let host = host_port
        .split(':')
        .next()
        .unwrap_or("")
        .trim_matches(|c| c == '[' || c == ']');
    if host.is_empty() {
        return Err(AuthStackError::validation("url host is missing"));
    }
    if host == "localhost" || host.ends_with(".localhost") {
        if !allow_private {
            return Err(AuthStackError::validation(
                "localhost targets are blocked (set AUTH_DASHBOARD_HTTP_ALLOW_PRIVATE=true to allow)",
            ));
        }
        return Ok(());
    }
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        let blocked = match ip {
            std::net::IpAddr::V4(v4) => {
                v4.is_private()
                    || v4.is_loopback()
                    || v4.is_link_local()
                    || v4.octets()[0] == 169 && v4.octets()[1] == 254
                    || v4.octets()[0] == 0
            }
            std::net::IpAddr::V6(v6) => v6.is_loopback() || v6.is_unique_local(),
        };
        if blocked && !allow_private {
            return Err(AuthStackError::validation(
                "private or link-local IP targets are blocked",
            ));
        }
    }
    Ok(())
}

pub(crate) fn default_notifications() -> Vec<crate::contracts::DashboardNotification> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    vec![
        crate::contracts::DashboardNotification {
            id: "n-welcome".to_owned(),
            title: "Welcome to your board".to_owned(),
            body: "Add, remove, and rearrange widgets. This layout is saved to your account."
                .to_owned(),
            level: "info".to_owned(),
            read: false,
            created_at_ms: ms,
        },
        crate::contracts::DashboardNotification {
            id: "n-security".to_owned(),
            title: "Harden sign-in".to_owned(),
            body: "Enroll an authenticator or passkey so step-up and phishing-resistant login are ready."
                .to_owned(),
            level: "warn".to_owned(),
            read: false,
            created_at_ms: ms.saturating_sub(60_000),
        },
    ]
}

pub(crate) fn validate_board_nodes(nodes: &[crate::contracts::BoardNode], depth: u8) -> AuthStackResult<()> {
    use crate::contracts::BoardNode;
    if depth > 4 {
        return Err(AuthStackError::validation(
            "dashboard containers can nest at most 4 levels",
        ));
    }
    for node in nodes {
        if node.id().trim().is_empty() || node.id().len() > 64 {
            return Err(AuthStackError::validation("node id is invalid"));
        }
        if !(1..=12).contains(&node.col_span()) {
            return Err(AuthStackError::validation("col_span must be 1–12"));
        }
        if let BoardNode::Container { children, .. } = node {
            validate_board_nodes(children, depth + 1)?;
        }
    }
    Ok(())
}

pub async fn load_dashboard_layout(
    org_id: &str,
) -> AuthStackResult<crate::contracts::DashboardLayout> {
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let store = profile_kv().await?;
        let key = dashboard_layout_key(org_id);
        let Some(bytes) = store
            .get(&key)
            .await
            .map_err(|error| AuthStackError::store(format!("dashboard layout read failed: {error}")))?
        else {
            let layout = default_dashboard_layout();
            save_dashboard_layout(org_id, &layout).await?;
            return Ok(layout);
        };
        match serde_json::from_slice::<crate::contracts::DashboardLayout>(&bytes) {
            Ok(mut layout) => {
                layout.migrate_if_needed();
                if layout.nodes.is_empty() {
                    let layout = default_dashboard_layout();
                    save_dashboard_layout(org_id, &layout).await?;
                    return Ok(layout);
                }
                // Persist migration so clients always see v2.
                if layout.version < 2 || !layout.widgets.is_empty() {
                    layout.widgets.clear();
                    layout.version = 2;
                    let _ = save_dashboard_layout(org_id, &layout).await;
                }
                Ok(layout)
            }
            _ => {
                let layout = default_dashboard_layout();
                save_dashboard_layout(org_id, &layout).await?;
                Ok(layout)
            }
        }
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = org_id;
        Ok(default_dashboard_layout())
    }
}

pub async fn save_dashboard_layout(
    org_id: &str,
    layout: &crate::contracts::DashboardLayout,
) -> AuthStackResult<()> {
    let mut layout = layout.clone();
    layout.migrate_if_needed();
    layout.widgets.clear();
    layout.version = 2;
    if layout.total_nodes() > MAX_BOARD_NODES {
        return Err(AuthStackError::validation(format!(
            "dashboard supports at most {MAX_BOARD_NODES} nodes"
        )));
    }
    if layout.nodes.is_empty() {
        return Err(AuthStackError::validation("dashboard must have at least one node"));
    }
    validate_board_nodes(&layout.nodes, 0)?;

    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let store = profile_kv().await?;
        let bytes = serde_json::to_vec(&layout)
            .map_err(|error| AuthStackError::serialization(error.to_string()))?;
        store
            .set(dashboard_layout_key(org_id), bytes)
            .await
            .map_err(|error| AuthStackError::store(format!("dashboard layout write failed: {error}")))
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = (org_id, layout);
        Err(AuthStackError::configuration(
            "dashboard storage requires Spin key-value",
        ))
    }
}

/// One-time: copy per-user board (layout/resources/queries) into org keys when org is empty.
pub async fn migrate_legacy_user_board_to_org(
    user_id: &str,
    org_id: &str,
) -> AuthStackResult<bool> {
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let store = profile_kv().await?;
        let mut changed = false;

        // Layout
        let org_layout_key = dashboard_layout_key(org_id);
        if store
            .get(&org_layout_key)
            .await
            .map_err(|e| AuthStackError::store(format!("layout read failed: {e}")))?
            .is_none()
        {
            if let Some(bytes) = store
                .get(dashboard_layout_legacy_user_key(user_id))
                .await
                .map_err(|e| AuthStackError::store(format!("legacy layout read failed: {e}")))?
            {
                store
                    .set(&org_layout_key, bytes)
                    .await
                    .map_err(|e| AuthStackError::store(format!("layout migrate write failed: {e}")))?;
                changed = true;
            }
        }

        // Resources
        let org_res_key = dashboard_resources_key(org_id);
        if store
            .get(&org_res_key)
            .await
            .map_err(|e| AuthStackError::store(format!("resources read failed: {e}")))?
            .is_none()
        {
            if let Some(bytes) = store
                .get(dashboard_resources_legacy_user_key(user_id))
                .await
                .map_err(|e| AuthStackError::store(format!("legacy resources read failed: {e}")))?
            {
                store
                    .set(&org_res_key, bytes)
                    .await
                    .map_err(|e| {
                        AuthStackError::store(format!("resources migrate write failed: {e}"))
                    })?;
                changed = true;
            }
        }

        // Queries
        let org_q_key = dashboard_queries_key(org_id);
        if store
            .get(&org_q_key)
            .await
            .map_err(|e| AuthStackError::store(format!("queries read failed: {e}")))?
            .is_none()
        {
            if let Some(bytes) = store
                .get(dashboard_queries_legacy_user_key(user_id))
                .await
                .map_err(|e| AuthStackError::store(format!("legacy queries read failed: {e}")))?
            {
                store
                    .set(&org_q_key, bytes)
                    .await
                    .map_err(|e| AuthStackError::store(format!("queries migrate write failed: {e}")))?;
                changed = true;
            }
        }

        Ok(changed)
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = (user_id, org_id);
        Ok(false)
    }
}

