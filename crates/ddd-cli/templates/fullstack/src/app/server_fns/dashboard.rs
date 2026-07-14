#![allow(unused_imports)]

use super::common::*;
use crate::contracts::*;
use leptos::prelude::*;
use server_fn::ServerFnError;
use server_fn::codec::Json;

#[server(prefix = "/api/ui")]
pub async fn get_dashboard_snapshot() -> Result<DashboardSnapshot, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::get_dashboard_snapshot(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

// Nested layout (u8 col_span, enums, tree) cannot use default PostUrl/serde_qs —
// it stringifies numbers ("3") and fails with "expected u8".
#[server(prefix = "/api/ui", input = Json)]
pub async fn save_dashboard_layout(
    layout: DashboardLayout,
) -> Result<DashboardLayout, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::save_dashboard_layout(
            crate::contracts::DashboardLayoutUpdate { layout },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = layout;
        unreachable!()
    }
}

// Includes numeric fields (cache_ttl_seconds); keep JSON for the same reason.
#[server(prefix = "/api/ui", input = Json)]
pub async fn upsert_dashboard_source(
    id: Option<String>,
    name: String,
    method: String,
    url: String,
    json_path: String,
    shape: String,
    cache_ttl_seconds: u32,
    body_template: Option<String>,
) -> Result<crate::contracts::DataSourceSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::upsert_dashboard_source(
            DataSourceUpsert {
                id,
                name,
                method,
                url,
                headers: Vec::new(),
                body_template,
                json_path,
                shape,
                cache_ttl_seconds,
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (id, name, method, url, json_path, shape, cache_ttl_seconds, body_template);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn delete_dashboard_source(source_id: String) -> Result<AcceptedResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::delete_dashboard_source(source_id, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = source_id;
        unreachable!()
    }
}

#[server(prefix = "/api/ui", input = Json)]
pub async fn create_dashboard_secret(
    org_slug: String,
    request: SecretCreateRequest,
) -> Result<crate::contracts::SecretSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::create_dashboard_secret(
            None,
            Some(org_slug),
            request,
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (org_slug, request);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn delete_dashboard_secret(
    org_slug: String,
    secret_id: String,
) -> Result<AcceptedResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::delete_dashboard_secret(
            None,
            Some(org_slug),
            secret_id,
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (org_slug, secret_id);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn reveal_dashboard_secret(
    org_slug: String,
    secret_id: String,
) -> Result<crate::contracts::SecretRevealResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::reveal_dashboard_secret(
            None,
            Some(org_slug),
            secret_id,
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (org_slug, secret_id);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_dashboard_secrets(
    org_slug: String,
) -> Result<Vec<crate::contracts::SecretSummary>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_dashboard_secrets(
            None,
            Some(org_slug),
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = org_slug;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn resolve_workspace_vault_target(
) -> Result<crate::contracts::OrganizationSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::resolve_workspace_vault_target(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn seed_dashboard_demos() -> Result<AcceptedResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::seed_dashboard_demos(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        unreachable!()
    }
}

#[server(prefix = "/api/ui", input = Json)]
pub async fn migrate_workspace_legacy_data(
    request: crate::contracts::WorkspaceLegacyMigrateRequest,
) -> Result<crate::contracts::WorkspaceLegacyMigrateReport, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::migrate_workspace_legacy_data(request, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = request;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn test_dashboard_http_source(
    source_id: String,
) -> Result<crate::contracts::HttpQueryResult, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::test_dashboard_http_source(source_id, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = source_id;
        unreachable!()
    }
}

#[server(prefix = "/api/ui", input = Json)]
pub async fn upsert_dashboard_resource(
    request: crate::contracts::ResourceUpsert,
) -> Result<crate::contracts::ResourceSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::upsert_dashboard_resource(request, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = request;
        unreachable!()
    }
}

#[server(prefix = "/api/ui", input = Json)]
pub async fn upsert_dashboard_query(
    request: crate::contracts::QueryUpsert,
) -> Result<crate::contracts::QuerySummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::upsert_dashboard_query(request, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = request;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn delete_dashboard_resource(
    resource_id: String,
) -> Result<AcceptedResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::delete_dashboard_resource(resource_id, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = resource_id;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn delete_dashboard_query(query_id: String) -> Result<AcceptedResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::delete_dashboard_query(query_id, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = query_id;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn run_dashboard_query(
    query_id: String,
) -> Result<crate::contracts::QueryResult, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::run_dashboard_query(query_id, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = query_id;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn dismiss_dashboard_notification(
    notification_id: String,
) -> Result<Vec<DashboardNotification>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::dismiss_dashboard_notification(
            notification_id,
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = notification_id;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn update_dashboard_note(
    widget_id: String,
    text: String,
) -> Result<DashboardLayout, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::update_dashboard_note(
            crate::contracts::DashboardNoteUpdate { widget_id, text },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (widget_id, text);
        unreachable!()
    }
}
