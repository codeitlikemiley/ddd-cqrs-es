#![allow(unused_imports)]
#![allow(dead_code)]

use std::sync::OnceLock;

use wasi_auth::authentication::Clock;
use wasi_auth::authentication::jwt::JwksDocument;
use wasi_auth::authorization::{
    AccessRequest, ActionName, Authorizer, MAX_BATCH_CHECKS, Resource, ResourceType,
};
use wasi_auth::cedar::{
    CedarError, CedarProvider, DEFAULT_APPLICATION_POLICY, DEFAULT_APPLICATION_POLICY_REVISION,
};
use wasi_auth::context::{
    AuthenticationAssurance, AuthorizationSnapshot, OrganizationId, PolicyRevision, Principal,
    RoleId, SessionId, UserId, VerifiedAuthContext, VerifiedRequestContext,
};
use wasi_auth::http::{
    AuthenticatedSession, Credential, CredentialAuthenticator, RoutePolicy, TrustedIngress,
    TrustedIngressConfig,
};

use super::*;
use crate::contracts::*;
use crate::error::{AuthStackError, AuthStackResult};

pub async fn get_dashboard_snapshot(auth: RequestAuth) -> AuthStackResult<DashboardSnapshot> {
    let authorization =
        require_workspace_permission(auth, "dashboard.view", AssuranceRequirement::Aal1).await?;
    let context = authorization.context;
    let session_id = context.session_id().as_str().to_owned();
    let session = crate::auth_product::get_session(Some(&session_id)).await?;
    let email = session.primary_email.clone();
    let greeting_name = email
        .as_deref()
        .and_then(|value| value.split('@').next())
        .filter(|value| !value.is_empty())
        .unwrap_or("there")
        .to_owned();
    let has_tenant = session
        .tenant_id
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty());

    let sessions = crate::auth_product::list_user_sessions(
        context.principal().user_id().as_str(),
        context.session_id().as_str(),
    )
    .await
    .map(|response| response.sessions)
    .unwrap_or_default();

    let organizations =
        crate::auth_product::list_organizations(context.principal().user_id().as_str())
            .await
            .map(|response| response.organizations)
            .unwrap_or_default();

    // Prefer human workspace name (+ slug) over raw UUID for UI labels.
    let tenant_label = session.tenant_id.as_ref().and_then(|tid| {
        organizations
            .iter()
            .find(|o| o.organization_id == *tid)
            .map(|o| {
                if o.slug.is_empty() {
                    o.name.clone()
                } else {
                    format!("{} · /org/{}", o.name, o.slug)
                }
            })
            .or_else(|| Some(tid.clone()))
    });

    let mfa = crate::auth_product::mfa_status(context.session_id().as_str())
        .await
        .ok();
    let totp_enrolled = mfa
        .as_ref()
        .map(|status| status.totp_enrolled)
        .unwrap_or(false);
    let recovery_codes_remaining = mfa
        .as_ref()
        .map(|status| status.recovery_codes_remaining)
        .unwrap_or(0);

    let activity = if has_tenant {
        crate::auth_product::list_audit_events(
            context.session_id().as_str(),
            session.tenant_id.as_deref(),
            0,
            12,
        )
        .await
        .map(|response| response.events)
        .unwrap_or_default()
    } else {
        Vec::new()
    };

    let notifications =
        crate::store::load_dashboard_notifications(&authorization.organization_id).await?;
    let sources = Vec::new();
    let vault_org_id = Some(authorization.organization_id);
    let organization = crate::auth_product::organization_for_session(
        context.session_id().as_str(),
        vault_org_id.as_deref().unwrap_or_default(),
    )
    .await?;
    let can_view_resources = organization
        .permissions
        .iter()
        .any(|permission| permission == "resource.view");
    let can_view_queries = organization
        .permissions
        .iter()
        .any(|permission| permission == "query.view");
    let can_execute_queries = organization
        .permissions
        .iter()
        .any(|permission| permission == "query.execute");
    let can_view_vault = organization
        .permissions
        .iter()
        .any(|permission| permission == "vault.view");
    // Workspace-scoped board: layout/resources/queries/secrets require selected tenant.
    let layout = match vault_org_id.as_deref() {
        Some(org) => crate::store::load_dashboard_layout(org).await?,
        None => crate::store::default_dashboard_layout_public(),
    };
    let secrets = match vault_org_id.as_deref() {
        Some(org) if can_view_vault => crate::store::list_secret_summaries(org)
            .await
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    let http_enabled = dashboard_http_enabled().await;
    let allow_private = dashboard_http_allow_private().await;

    let mut security_score: u8 = 35;
    if totp_enrolled {
        security_score = security_score.saturating_add(35);
    }
    if recovery_codes_remaining > 0 {
        security_score = security_score.saturating_add(15);
    }
    if sessions.len() <= 3 {
        security_score = security_score.saturating_add(15);
    }
    if session.assurance.to_ascii_lowercase().contains("aal2")
        || session.assurance.to_ascii_lowercase().contains("2")
    {
        security_score = security_score.saturating_add(10);
    }
    security_score = security_score.min(100);

    let mut placed = std::collections::HashSet::new();
    fn collect_kinds(nodes: &[BoardNode], placed: &mut std::collections::HashSet<String>) {
        for node in nodes {
            match node {
                BoardNode::Widget { kind, .. } => {
                    placed.insert(kind.as_str().to_owned());
                }
                BoardNode::Container { children, .. } => collect_kinds(children, placed),
            }
        }
    }
    collect_kinds(&layout.nodes, &mut placed);

    let catalog = DashboardWidgetKind::catalog()
        .iter()
        .map(|kind| DashboardCatalogItem {
            already_added: !kind.allows_multiple() && placed.contains(kind.as_str()),
            allows_multiple: kind.allows_multiple(),
            default_span: kind.default_span(),
            description: kind.description().to_owned(),
            kind: kind.clone(),
            label: kind.label().to_owned(),
        })
        .collect();

    let data_sources: Vec<DataSourceSummary> = sources
        .iter()
        .map(crate::store::data_source_to_summary)
        .collect();

    let resources = match vault_org_id.as_deref() {
        Some(org) if can_view_resources => {
            crate::store::load_resources(org).await.unwrap_or_default()
        }
        _ => Vec::new(),
    };
    let queries = match vault_org_id.as_deref() {
        Some(org) if can_view_queries => crate::store::load_queries(org).await.unwrap_or_default(),
        _ => Vec::new(),
    };
    let resource_summaries: Vec<crate::contracts::ResourceSummary> = resources
        .iter()
        .map(crate::store::resource_to_summary)
        .collect();
    let query_summaries: Vec<crate::contracts::QuerySummary> = queries
        .iter()
        .map(|q| crate::store::query_to_summary(q, &resources))
        .collect();

    // Execute queries referenced by bound board widgets (capped).
    let mut http_source_ids = Vec::new();
    fn collect_query_ids(nodes: &[BoardNode], out: &mut Vec<String>) {
        for node in nodes {
            match node {
                BoardNode::Widget {
                    kind,
                    source_id: Some(id),
                    ..
                } if kind.is_query_bound() => {
                    if !out.contains(id) {
                        out.push(id.clone());
                    }
                }
                BoardNode::Container { children, .. } => collect_query_ids(children, out),
                _ => {}
            }
        }
    }
    collect_query_ids(&layout.nodes, &mut http_source_ids);
    http_source_ids.truncate(8);
    let mut http_results = Vec::new();
    let mut query_results = Vec::new();
    if http_enabled {
        for source_id in http_source_ids {
            // Prefer QueryResult path (also feeds legacy HttpQueryResult for HttpPanel).
            let Some(org) = vault_org_id.as_deref() else {
                continue;
            };
            let Some(query) = queries.iter().find(|query| query.id == source_id) else {
                continue;
            };
            if !can_execute_queries
                || query.config.execution_class() != crate::contracts::QueryExecutionClass::Read
            {
                continue;
            }
            match crate::store::execute_dashboard_query(org, &source_id, allow_private).await {
                Ok(result) => {
                    http_results.push(HttpQueryResult {
                        source_id: result.query_id.clone(),
                        ok: result.ok,
                        error: result.error.clone(),
                        data_json: result.data_json.clone(),
                        display_mode: crate::contracts::HttpDisplayMode::List,
                    });
                    query_results.push(result);
                }
                Err(error) => {
                    http_results.push(HttpQueryResult {
                        source_id: source_id.clone(),
                        ok: false,
                        error: Some(error.public_message()),
                        data_json: "null".to_owned(),
                        display_mode: crate::contracts::HttpDisplayMode::List,
                    });
                    query_results.push(crate::contracts::QueryResult::err(
                        source_id,
                        crate::contracts::ResourceKind::Rest,
                        error.public_message(),
                    ));
                }
            }
        }
    }

    let grpc_resources_enabled = dashboard_grpc_enabled().await;
    let postgres_resources_enabled = true;

    Ok(DashboardSnapshot {
        greeting_name,
        email,
        assurance: session.assurance,
        has_tenant,
        tenant_label,
        system_administrator: session.system_administrator,
        organization_count: organizations.len() as u32,
        active_session_count: sessions.len() as u32,
        security_score,
        totp_enrolled,
        recovery_codes_remaining,
        sessions,
        organizations,
        activity,
        notifications,
        layout,
        catalog,
        data_sources,
        secrets,
        http_results,
        http_enabled,
        resources: resource_summaries,
        queries: query_summaries,
        query_results,
        postgres_resources_enabled,
        grpc_resources_enabled,
    })
}

pub(crate) async fn dashboard_grpc_enabled() -> bool {
    matches!(
        config_value("AUTH_DASHBOARD_GRPC_ENABLED")
            .await
            .as_deref()
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

pub(crate) async fn dashboard_http_enabled() -> bool {
    !matches!(
        config_value("AUTH_DASHBOARD_HTTP_ENABLED")
            .await
            .as_deref()
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("0" | "false" | "no" | "off")
    )
}

pub(crate) async fn dashboard_http_allow_private() -> bool {
    matches!(
        config_value("AUTH_DASHBOARD_HTTP_ALLOW_PRIVATE")
            .await
            .as_deref()
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

/// Require one live workspace permission and return `(user_id, org_id)`.
pub(crate) async fn require_workspace_board(
    auth: RequestAuth,
    permission: &str,
    assurance: AssuranceRequirement,
) -> AuthStackResult<(String, String)> {
    let authorization = require_workspace_permission(auth, permission, assurance).await?;
    Ok((authorization.user_id, authorization.organization_id))
}

pub async fn save_dashboard_layout(
    request: DashboardLayoutUpdate,
    auth: RequestAuth,
) -> AuthStackResult<DashboardLayout> {
    let (_user_id, org_id) =
        require_workspace_board(auth, "dashboard.manage", AssuranceRequirement::Aal1).await?;
    let mut layout = request.layout;
    layout.migrate_if_needed();
    crate::store::save_dashboard_layout(&org_id, &layout).await?;
    crate::store::load_dashboard_layout(&org_id).await
}

pub async fn dismiss_dashboard_notification(
    notification_id: String,
    auth: RequestAuth,
) -> AuthStackResult<Vec<DashboardNotification>> {
    if notification_id.trim().is_empty() {
        return Err(AuthStackError::validation("notification_id is required"));
    }
    let authorization =
        require_workspace_permission(auth, "dashboard.view", AssuranceRequirement::Aal1).await?;
    crate::store::dismiss_dashboard_notification(
        &authorization.organization_id,
        notification_id.trim(),
    )
    .await
}

pub async fn update_dashboard_note(
    request: DashboardNoteUpdate,
    auth: RequestAuth,
) -> AuthStackResult<DashboardLayout> {
    if request.widget_id.trim().is_empty() {
        return Err(AuthStackError::validation("widget_id is required"));
    }
    if request.text.chars().count() > 2_000 {
        return Err(AuthStackError::validation("note is too long"));
    }
    let (_user_id, org_id) =
        require_workspace_board(auth, "dashboard.manage", AssuranceRequirement::Aal1).await?;
    let mut layout = crate::store::load_dashboard_layout(&org_id).await?;
    let Some(node) = layout.find_widget_mut(request.widget_id.trim()) else {
        return Err(AuthStackError::not_found("widget not found"));
    };
    match node {
        BoardNode::Widget {
            kind: DashboardWidgetKind::Notes,
            note_text,
            ..
        } => {
            *note_text = Some(request.text);
        }
        _ => return Err(AuthStackError::validation("widget is not a notes tile")),
    }
    crate::store::save_dashboard_layout(&org_id, &layout).await?;
    crate::store::load_dashboard_layout(&org_id).await
}

pub async fn list_dashboard_sources(auth: RequestAuth) -> AuthStackResult<Vec<DataSourceSummary>> {
    require_workspace_permission(auth, "resource.view", AssuranceRequirement::Aal1).await?;
    Ok(Vec::new())
}

pub async fn upsert_dashboard_source(
    request: DataSourceUpsert,
    auth: RequestAuth,
) -> AuthStackResult<DataSourceSummary> {
    if !dashboard_http_enabled().await {
        return Err(AuthStackError::configuration(
            "HTTP dashboard sources are disabled",
        ));
    }
    let _ = request;
    require_workspace_permission(auth, "resource.manage", AssuranceRequirement::Aal2).await?;
    Err(AuthStackError::not_found(
        "legacy data-source API was removed; use workspace resources",
    ))
}

pub async fn delete_dashboard_source(
    source_id: String,
    auth: RequestAuth,
) -> AuthStackResult<AcceptedResponse> {
    let _ = source_id;
    require_workspace_permission(auth, "resource.manage", AssuranceRequirement::Aal2).await?;
    Err(AuthStackError::not_found(
        "legacy data-source API was removed; use workspace resources",
    ))
}

pub async fn test_dashboard_http_source(
    source_id: String,
    auth: RequestAuth,
) -> AuthStackResult<HttpQueryResult> {
    if !dashboard_http_enabled().await {
        return Err(AuthStackError::configuration(
            "HTTP dashboard sources are disabled",
        ));
    }
    let result = run_dashboard_query(source_id, auth).await?;
    Ok(HttpQueryResult {
        source_id: result.query_id,
        ok: result.ok,
        error: result.error,
        data_json: result.data_json,
        display_mode: crate::contracts::HttpDisplayMode::List,
    })
}

pub async fn upsert_dashboard_resource(
    request: crate::contracts::ResourceUpsert,
    auth: RequestAuth,
) -> AuthStackResult<crate::contracts::ResourceSummary> {
    if matches!(request.kind, crate::contracts::ResourceKind::Rest)
        && !dashboard_http_enabled().await
    {
        return Err(AuthStackError::configuration(
            "HTTP dashboard resources are disabled",
        ));
    }
    if matches!(request.kind, crate::contracts::ResourceKind::Grpc)
        && !dashboard_grpc_enabled().await
    {
        return Err(AuthStackError::configuration(
            "gRPC dashboard resources are disabled (set AUTH_DASHBOARD_GRPC_ENABLED=true)",
        ));
    }
    let (_user_id, org_id) =
        require_workspace_board(auth, "resource.manage", AssuranceRequirement::Aal2).await?;
    crate::store::upsert_resource(&org_id, request, dashboard_http_allow_private().await).await
}

pub async fn upsert_dashboard_query(
    request: crate::contracts::QueryUpsert,
    auth: RequestAuth,
) -> AuthStackResult<crate::contracts::QuerySummary> {
    let (_user_id, org_id) =
        require_workspace_board(auth, "query.manage", AssuranceRequirement::Aal2).await?;
    crate::store::upsert_query(&org_id, request).await
}

pub async fn delete_dashboard_resource(
    resource_id: String,
    auth: RequestAuth,
) -> AuthStackResult<AcceptedResponse> {
    let (_user_id, org_id) =
        require_workspace_board(auth, "resource.manage", AssuranceRequirement::Aal2).await?;
    crate::store::delete_resource(&org_id, resource_id.trim()).await?;
    Ok(AcceptedResponse { accepted: true })
}

pub async fn delete_dashboard_query(
    query_id: String,
    auth: RequestAuth,
) -> AuthStackResult<AcceptedResponse> {
    let (_user_id, org_id) =
        require_workspace_board(auth, "query.manage", AssuranceRequirement::Aal2).await?;
    crate::store::delete_query(&org_id, query_id.trim()).await?;
    Ok(AcceptedResponse { accepted: true })
}

pub async fn run_dashboard_query(
    query_id: String,
    auth: RequestAuth,
) -> AuthStackResult<crate::contracts::QueryResult> {
    let authorization =
        require_workspace_permission(auth, "dashboard.view", AssuranceRequirement::Aal1).await?;
    let org_id = authorization.organization_id.clone();
    let queries = crate::store::load_queries(&org_id).await?;
    let query = queries
        .iter()
        .find(|query| query.id == query_id.trim())
        .ok_or_else(|| AuthStackError::not_found("query not found"))?;
    let (permission, assurance) = match query.config.execution_class() {
        crate::contracts::QueryExecutionClass::Read => {
            ("query.execute", AssuranceRequirement::Aal1)
        }
        crate::contracts::QueryExecutionClass::Mutation => {
            ("query.execute_mutation", AssuranceRequirement::Aal2)
        }
    };
    require_organization_permission(&authorization.context, &org_id, permission, assurance).await?;
    crate::store::execute_dashboard_query(
        &org_id,
        query_id.trim(),
        dashboard_http_allow_private().await,
    )
    .await
}
