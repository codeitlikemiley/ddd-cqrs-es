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

/// Idempotent demo pack: REST + @app Postgres resources/queries + bound widgets.
/// Writes into the **workspace** board + vault (`org_id`).
/// Returns `true` when something new was seeded.
pub async fn seed_dashboard_demos(org_id: &str) -> AuthStackResult<bool> {
    use crate::contracts::{
        BoardContainerKind, BoardNode, DashboardQuery, DashboardResource, DashboardWidgetKind,
        HttpDisplayMode, HttpMethod, QueryConfig, ResourceAuth, ResourceConfig, ResourceKind,
        TransformStep, WidgetBind,
    };

    const DEMO_REST_RES: &str = "demo-res-jsonplaceholder";
    const DEMO_PG_RES: &str = "demo-res-app-postgres";
    const DEMO_Q_LIST: &str = "demo-q-todos";
    const DEMO_Q_METRIC: &str = "demo-q-todo-count";
    const DEMO_Q_TABLE: &str = "demo-q-pg-info";
    const DEMO_ROW: &str = "demo-row-connectors";
    const DEMO_W_LIST: &str = "demo-w-list";
    const DEMO_W_METRIC: &str = "demo-w-metric";
    const DEMO_W_TABLE: &str = "demo-w-table";

    let mut resources = load_resources(org_id).await.unwrap_or_default();
    let mut queries = load_queries(org_id).await.unwrap_or_default();
    let mut layout = load_dashboard_layout(org_id).await?;
    layout.migrate_if_needed();
    let mut changed = false;

    // Placeholder vault secret for demos that show how auth pickers work.
    let secrets = load_secrets_raw(org_id).await?;
    if !secrets
        .iter()
        .any(|s| secret_display_key(s) == "DEMO_API_TOKEN")
    {
        let _ = create_secret(
            org_id,
            &crate::contracts::SecretCreateRequest {
                key: "DEMO_API_TOKEN".to_owned(),
                name: "DEMO_API_TOKEN".to_owned(),
                value: "demo-not-secret".to_owned(),
                label: "Demo API token".to_owned(),
                description: "Placeholder for resource auth pickers — not a real credential."
                    .to_owned(),
                scope: "user".to_owned(),
            },
        )
        .await?;
        changed = true;
    }

    if !resources.iter().any(|r| r.id == DEMO_REST_RES) {
        resources.push(DashboardResource {
            id: DEMO_REST_RES.to_owned(),
            name: "Demo · JSONPlaceholder".to_owned(),
            kind: ResourceKind::Rest,
            auth: ResourceAuth::None,
            default_headers: Vec::new(),
            config: ResourceConfig::Rest {
                base_url: "https://jsonplaceholder.typicode.com".to_owned(),
                timeout_ms: 15_000,
            },
        });
        changed = true;
    }

    if !resources.iter().any(|r| r.id == DEMO_PG_RES) {
        resources.push(DashboardResource {
            id: DEMO_PG_RES.to_owned(),
            name: "Demo · App Postgres".to_owned(),
            kind: ResourceKind::Postgres,
            auth: ResourceAuth::None,
            default_headers: Vec::new(),
            config: ResourceConfig::Postgres {
                host: "@app".to_owned(),
                port: 5432,
                database: String::new(),
                user: String::new(),
                password_secret_id: String::new(),
                ssl_mode: crate::contracts::PostgresSslMode::Prefer,
            },
        });
        changed = true;
    }

    if !queries.iter().any(|q| q.id == DEMO_Q_LIST) {
        queries.push(DashboardQuery {
            id: DEMO_Q_LIST.to_owned(),
            name: "Demo todos".to_owned(),
            resource_id: DEMO_REST_RES.to_owned(),
            transform: vec![
                TransformStep::AsArray,
                TransformStep::Limit { n: 5 },
            ],
            config: QueryConfig::Rest {
                method: HttpMethod::Get,
                path: "/todos".to_owned(),
                query_params: Vec::new(),
                headers: Vec::new(),
                body: None,
            },
        });
        changed = true;
    }

    if !queries.iter().any(|q| q.id == DEMO_Q_METRIC) {
        queries.push(DashboardQuery {
            id: DEMO_Q_METRIC.to_owned(),
            name: "Demo todo #1".to_owned(),
            resource_id: DEMO_REST_RES.to_owned(),
            transform: Vec::new(),
            config: QueryConfig::Rest {
                method: HttpMethod::Get,
                path: "/todos/1".to_owned(),
                query_params: Vec::new(),
                headers: Vec::new(),
                body: None,
            },
        });
        changed = true;
    }

    if !queries.iter().any(|q| q.id == DEMO_Q_TABLE) {
        queries.push(DashboardQuery {
            id: DEMO_Q_TABLE.to_owned(),
            name: "Demo pg info".to_owned(),
            resource_id: DEMO_PG_RES.to_owned(),
            transform: Vec::new(),
            config: QueryConfig::Postgres {
                sql: "SELECT current_user AS user_name, current_database() AS db, now()::text AS ts"
                    .to_owned(),
            },
        });
        changed = true;
    }

    if changed {
        save_resources(org_id, &resources).await?;
        save_queries(org_id, &queries).await?;
    }

    let has_demo_row = layout.nodes.iter().any(|n| n.id() == DEMO_ROW);
    if !has_demo_row {
        layout.nodes.insert(
            0,
            BoardNode::Container {
                id: DEMO_ROW.to_owned(),
                kind: BoardContainerKind::Row,
                col_span: 12,
                children: vec![
                    BoardNode::Widget {
                        id: DEMO_W_LIST.to_owned(),
                        kind: DashboardWidgetKind::BoundList,
                        col_span: 6,
                        note_text: None,
                        source_id: Some(DEMO_Q_LIST.to_owned()),
                        bind: WidgetBind {
                            title_path: Some("title".to_owned()),
                            subtitle_path: Some("id".to_owned()),
                            meta_path: Some("completed".to_owned()),
                            ..WidgetBind::default()
                        },
                        http_mode: HttpDisplayMode::List,
                    },
                    BoardNode::Widget {
                        id: DEMO_W_METRIC.to_owned(),
                        kind: DashboardWidgetKind::BoundMetric,
                        col_span: 3,
                        note_text: None,
                        source_id: Some(DEMO_Q_METRIC.to_owned()),
                        bind: WidgetBind {
                            value_path: Some("id".to_owned()),
                            label_path: Some("title".to_owned()),
                            ..WidgetBind::default()
                        },
                        http_mode: HttpDisplayMode::Metric,
                    },
                    BoardNode::Widget {
                        id: DEMO_W_TABLE.to_owned(),
                        kind: DashboardWidgetKind::BoundTable,
                        col_span: 3,
                        note_text: None,
                        source_id: Some(DEMO_Q_TABLE.to_owned()),
                        bind: WidgetBind::default(),
                        http_mode: HttpDisplayMode::Table,
                    },
                ],
            },
        );
        save_dashboard_layout(org_id, &layout).await?;
        changed = true;
    }

    Ok(changed)
}

// ─── Resources / Queries (Retool model) ──────────────────────────────────────

