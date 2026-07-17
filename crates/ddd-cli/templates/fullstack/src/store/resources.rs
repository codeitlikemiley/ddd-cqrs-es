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

pub async fn load_resources(
    org_id: &str,
) -> AuthStackResult<Vec<crate::contracts::DashboardResource>> {
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let rows = execute_postgres(
            "SELECT payload FROM fullstack_app.resources \
             WHERE organization_id = ?1::text::uuid ORDER BY resource_id",
            vec![Value::String(org_id.to_owned())],
        )
        .await?;
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
        Ok(Vec::new())
    }
}

pub async fn save_resources(
    org_id: &str,
    resources: &[crate::contracts::DashboardResource],
) -> AuthStackResult<()> {
    if resources.len() > MAX_RESOURCES {
        return Err(AuthStackError::validation(format!(
            "at most {MAX_RESOURCES} resources"
        )));
    }
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let payload = serde_json::to_value(resources)
            .map_err(|error| AuthStackError::serialization(error.to_string()))?;
        execute_postgres(
            "WITH incoming AS ( \
                 SELECT item->>'id' AS resource_id, item AS payload \
                 FROM jsonb_array_elements(?1::text::jsonb) AS item \
             ), deleted AS ( \
                 DELETE FROM fullstack_app.resources existing \
                 WHERE existing.organization_id = ?2::text::uuid \
                   AND NOT EXISTS (SELECT 1 FROM incoming WHERE incoming.resource_id = existing.resource_id) \
             ) \
             INSERT INTO fullstack_app.resources \
                 (organization_id, resource_id, payload) \
             SELECT ?2::text::uuid, resource_id, payload FROM incoming \
             ON CONFLICT (organization_id, resource_id) DO UPDATE SET \
                 payload = EXCLUDED.payload, \
                 revision = fullstack_app.resources.revision + 1, \
                 updated_at = CURRENT_TIMESTAMP",
            vec![payload, Value::String(org_id.to_owned())],
        )
        .await
        .map(|_| ())
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = (org_id, resources);
        Err(AuthStackError::configuration(
            "dashboard storage requires Spin key-value",
        ))
    }
}

pub async fn load_queries(org_id: &str) -> AuthStackResult<Vec<crate::contracts::DashboardQuery>> {
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let rows = execute_postgres(
            "SELECT payload FROM fullstack_app.queries \
             WHERE organization_id = ?1::text::uuid ORDER BY query_id",
            vec![Value::String(org_id.to_owned())],
        )
        .await?;
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
        Ok(Vec::new())
    }
}

pub async fn save_queries(
    org_id: &str,
    queries: &[crate::contracts::DashboardQuery],
) -> AuthStackResult<()> {
    if queries.len() > MAX_QUERIES {
        return Err(AuthStackError::validation(format!(
            "at most {MAX_QUERIES} queries"
        )));
    }
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let payload = serde_json::to_value(queries)
            .map_err(|error| AuthStackError::serialization(error.to_string()))?;
        execute_postgres(
            "WITH incoming AS ( \
                 SELECT item->>'id' AS query_id, item->>'resource_id' AS resource_id, item AS payload \
                 FROM jsonb_array_elements(?1::text::jsonb) AS item \
             ), deleted AS ( \
                 DELETE FROM fullstack_app.queries existing \
                 WHERE existing.organization_id = ?2::text::uuid \
                   AND NOT EXISTS (SELECT 1 FROM incoming WHERE incoming.query_id = existing.query_id) \
             ) \
             INSERT INTO fullstack_app.queries \
                 (organization_id, query_id, resource_id, payload) \
             SELECT ?2::text::uuid, query_id, resource_id, payload FROM incoming \
             ON CONFLICT (organization_id, query_id) DO UPDATE SET \
                 resource_id = EXCLUDED.resource_id, \
                 payload = EXCLUDED.payload, \
                 revision = fullstack_app.queries.revision + 1, \
                 updated_at = CURRENT_TIMESTAMP",
            vec![payload, Value::String(org_id.to_owned())],
        )
        .await
        .map(|_| ())
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = (org_id, queries);
        Err(AuthStackError::configuration(
            "dashboard storage requires Spin key-value",
        ))
    }
}

/// Legacy HTTP `DataSource` → Resource/Query migration is no longer automatic
/// under org-scoped boards (sources stayed per-user). New orgs start empty.
pub(crate) async fn migrate_legacy_sources_to_resources(
    org_id: &str,
) -> AuthStackResult<Vec<crate::contracts::DashboardResource>> {
    let _ = org_id;
    Ok(Vec::new())
}

pub(crate) fn split_base_and_path(url: &str) -> (String, String) {
    let url = url.trim();
    // scheme://host[:port]/path]
    let after_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"));
    let Some(rest) = after_scheme else {
        return (url.to_owned(), "/".to_owned());
    };
    let scheme = if url.starts_with("https://") {
        "https"
    } else {
        "http"
    };
    let (host_port, path) = match rest.find('/') {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, "/"),
    };
    if host_port.is_empty() {
        return (url.to_owned(), "/".to_owned());
    }
    (format!("{scheme}://{host_port}"), path.to_owned())
}

pub fn resource_to_summary(
    resource: &crate::contracts::DashboardResource,
) -> crate::contracts::ResourceSummary {
    use crate::contracts::{ResourceAuth, ResourceConfig};
    let auth_type = match &resource.auth {
        ResourceAuth::None => "none",
        ResourceAuth::Bearer { .. } => "bearer",
        ResourceAuth::Basic { .. } => "basic",
        ResourceAuth::ApiKey { .. } => "api_key",
        ResourceAuth::OAuth2ClientCredentials { .. } => "oauth2_cc",
    }
    .to_owned();
    let detail = match &resource.config {
        ResourceConfig::Builtin => "Built-in app data".to_owned(),
        ResourceConfig::Rest { base_url, .. } => base_url.clone(),
        ResourceConfig::Postgres {
            host,
            port,
            database,
            ..
        } => format!("{host}:{port}/{database}"),
        ResourceConfig::Grpc {
            host,
            port,
            gateway_base_url,
            ..
        } => {
            if let Some(g) = gateway_base_url.as_ref().filter(|s| !s.is_empty()) {
                format!("gateway:{g}")
            } else {
                format!("grpc://{host}:{port}")
            }
        }
    };
    let has_secrets = !matches!(resource.auth, ResourceAuth::None)
        || resource
            .default_headers
            .iter()
            .any(|h| matches!(h.value, crate::contracts::HeaderValue::Secret { .. }))
        || matches!(
            &resource.config,
            ResourceConfig::Postgres {
                password_secret_id, ..
            } if !password_secret_id.is_empty()
        );
    crate::contracts::ResourceSummary {
        id: resource.id.clone(),
        name: resource.name.clone(),
        kind: resource.kind.clone(),
        auth_type,
        detail,
        header_names: resource
            .default_headers
            .iter()
            .map(|h| h.name.clone())
            .collect(),
        has_secrets,
    }
}

pub fn query_to_summary(
    query: &crate::contracts::DashboardQuery,
    resources: &[crate::contracts::DashboardResource],
) -> crate::contracts::QuerySummary {
    use crate::contracts::{QueryConfig, ResourceKind};
    let kind = resources
        .iter()
        .find(|r| r.id == query.resource_id)
        .map(|r| r.kind.clone())
        .unwrap_or(ResourceKind::Rest);
    let detail = match &query.config {
        QueryConfig::Rest { method, path, .. } => format!("{} {path}", method.as_str()),
        QueryConfig::Postgres { sql } => {
            let one = sql.lines().next().unwrap_or("SQL").trim();
            if one.len() > 64 {
                format!("{}…", &one[..64])
            } else {
                one.to_owned()
            }
        }
        QueryConfig::Grpc {
            service, method, ..
        } => format!("{service}/{method}"),
        QueryConfig::Builtin { key } => key.as_str().to_owned(),
    };
    crate::contracts::QuerySummary {
        id: query.id.clone(),
        name: query.name.clone(),
        resource_id: query.resource_id.clone(),
        resource_kind: kind,
        detail,
    }
}

/// Resolve a HeaderValue against the secret vault.
pub fn resolve_header_value(
    value: &crate::contracts::HeaderValue,
    secrets: &[StoredSecret],
) -> AuthStackResult<String> {
    match value {
        crate::contracts::HeaderValue::Literal { value } => Ok(value.clone()),
        crate::contracts::HeaderValue::Secret { secret_id } => secrets
            .iter()
            .find(|s| s.id == *secret_id)
            .map(|s| s.value.clone())
            .ok_or_else(|| AuthStackError::validation(format!("missing secret {secret_id}"))),
    }
}

/// Merge resource defaults + query overrides (query wins on name, case-insensitive).
pub fn merge_headers(
    resource_headers: &[crate::contracts::HeaderBag],
    query_headers: &[crate::contracts::HeaderBag],
) -> Vec<crate::contracts::HeaderBag> {
    let mut map: std::collections::BTreeMap<String, crate::contracts::HeaderBag> =
        std::collections::BTreeMap::new();
    for h in resource_headers {
        map.insert(h.name.to_ascii_lowercase(), h.clone());
    }
    for h in query_headers {
        map.insert(h.name.to_ascii_lowercase(), h.clone());
    }
    map.into_values().collect()
}

/// Apply ResourceAuth injectors into resolved header list (name, value).
pub fn apply_resource_auth(
    auth: &crate::contracts::ResourceAuth,
    secrets: &[StoredSecret],
    headers: &mut Vec<(String, String)>,
    query_params: &mut Vec<(String, String)>,
) -> AuthStackResult<()> {
    use crate::contracts::{ApiKeyLocation, ResourceAuth};
    match auth {
        ResourceAuth::None => {}
        ResourceAuth::Bearer { secret_id } => {
            let token = secrets
                .iter()
                .find(|s| s.id == *secret_id)
                .map(|s| s.value.clone())
                .ok_or_else(|| AuthStackError::validation("bearer secret missing"))?;
            if !headers
                .iter()
                .any(|(n, _)| n.eq_ignore_ascii_case("authorization"))
            {
                headers.push(("Authorization".to_owned(), format!("Bearer {token}")));
            }
        }
        ResourceAuth::Basic {
            username,
            password_secret_id,
        } => {
            let password = secrets
                .iter()
                .find(|s| s.id == *password_secret_id)
                .map(|s| s.value.clone())
                .ok_or_else(|| AuthStackError::validation("basic password secret missing"))?;
            let encoded = base64_encode(&format!("{username}:{password}"));
            if !headers
                .iter()
                .any(|(n, _)| n.eq_ignore_ascii_case("authorization"))
            {
                headers.push(("Authorization".to_owned(), format!("Basic {encoded}")));
            }
        }
        ResourceAuth::ApiKey {
            location,
            name,
            secret_id,
        } => {
            let key = secrets
                .iter()
                .find(|s| s.id == *secret_id)
                .map(|s| s.value.clone())
                .ok_or_else(|| AuthStackError::validation("api key secret missing"))?;
            match location {
                ApiKeyLocation::Header => {
                    if !headers.iter().any(|(n, _)| n.eq_ignore_ascii_case(name)) {
                        headers.push((name.clone(), key));
                    }
                }
                ApiKeyLocation::QueryParam => {
                    if !query_params.iter().any(|(n, _)| n == name) {
                        query_params.push((name.clone(), key));
                    }
                }
            }
        }
        ResourceAuth::OAuth2ClientCredentials { .. } => {
            // Token fetch happens in execute path; placeholder rejects until wired.
            return Err(AuthStackError::validation(
                "OAuth2 client credentials execution is not enabled yet",
            ));
        }
    }
    Ok(())
}

pub(crate) fn base64_encode(input: &str) -> String {
    #[cfg(feature = "ssr")]
    {
        use base64::Engine;
        return base64::engine::general_purpose::STANDARD.encode(input.as_bytes());
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = input;
        String::new()
    }
}

/// Declarative transform pipeline (json_path → as_array → map_fields → limit → pick_scalar).
pub fn apply_transform_pipeline(value: Value, steps: &[crate::contracts::TransformStep]) -> Value {
    use crate::app::dashboard::bind::json_path_get;
    use crate::contracts::TransformStep;
    let mut current = value;
    for step in steps {
        current = match step {
            TransformStep::JsonPath { path } => {
                json_path_get(&current, path).unwrap_or(Value::Null)
            }
            TransformStep::AsArray => match current {
                Value::Array(_) => current,
                Value::Object(map) => Value::Array(map.into_iter().map(|(_, v)| v).collect()),
                Value::Null => Value::Array(vec![]),
                other => Value::Array(vec![other]),
            },
            TransformStep::MapFields { fields } => {
                let rows: Vec<Value> = match &current {
                    Value::Array(items) => items.clone(),
                    other => vec![other.clone()],
                };
                let mapped: Vec<Value> = rows
                    .into_iter()
                    .map(|row| {
                        let mut obj = serde_json::Map::new();
                        for (target, source_path) in fields {
                            if let Some(v) = json_path_get(&row, source_path) {
                                obj.insert(target.clone(), v);
                            }
                        }
                        Value::Object(obj)
                    })
                    .collect();
                Value::Array(mapped)
            }
            TransformStep::Limit { n } => match current {
                Value::Array(mut items) => {
                    items.truncate(*n as usize);
                    Value::Array(items)
                }
                other => other,
            },
            TransformStep::PickScalar { path } => {
                if path.trim().is_empty() {
                    current
                } else {
                    json_path_get(&current, path).unwrap_or(Value::Null)
                }
            }
        };
    }
    current
}

pub fn data_source_to_summary(
    source: &crate::contracts::DataSource,
) -> crate::contracts::DataSourceSummary {
    crate::contracts::DataSourceSummary {
        id: source.id.clone(),
        name: source.name.clone(),
        kind: source.kind.clone(),
        builtin_key: source.builtin_key.clone(),
        method: source.method.clone(),
        url: source.url.clone(),
        json_path: source.json_path.clone(),
        shape: source.shape.clone(),
        header_names: source.headers.iter().map(|h| h.name.clone()).collect(),
        has_secrets: source.headers.iter().any(|h| h.secret_id.is_some()),
    }
}

pub async fn upsert_http_source(
    user_id: &str,
    request: crate::contracts::DataSourceUpsert,
    allow_private: bool,
) -> AuthStackResult<crate::contracts::DataSourceSummary> {
    validate_http_url(&request.url, allow_private)?;
    let method = request.method.trim().to_ascii_uppercase();
    if method != "GET" && method != "POST" {
        return Err(AuthStackError::validation("method must be GET or POST"));
    }
    let name = request.name.trim();
    if name.is_empty() || name.len() > 80 {
        return Err(AuthStackError::validation("name is invalid"));
    }
    let shape = request.shape.trim().to_ascii_lowercase();
    if shape != "one" && shape != "list" {
        return Err(AuthStackError::validation("shape must be one or list"));
    }
    let mut sources = load_data_sources(user_id).await?;
    let id = request
        .id
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| new_id("ds"));
    let source = crate::contracts::DataSource {
        id: id.clone(),
        name: name.to_owned(),
        kind: crate::contracts::DataSourceKind::Http,
        builtin_key: None,
        method,
        url: request.url.trim().to_owned(),
        headers: request.headers,
        body_template: request.body_template,
        json_path: request.json_path.trim().to_owned(),
        shape,
        cache_ttl_seconds: request.cache_ttl_seconds.min(3_600).max(0),
    };
    if let Some(existing) = sources.iter_mut().find(|s| s.id == id) {
        *existing = source.clone();
    } else {
        if sources.len() >= MAX_HTTP_SOURCES {
            return Err(AuthStackError::validation("too many sources"));
        }
        sources.push(source.clone());
    }
    save_data_sources(user_id, &sources).await?;
    // Legacy HTTP sources stay per-user; org board resources are edited via Resources modal.
    Ok(data_source_to_summary(&source))
}

pub async fn upsert_resource(
    org_id: &str,
    request: crate::contracts::ResourceUpsert,
    allow_private: bool,
) -> AuthStackResult<crate::contracts::ResourceSummary> {
    use crate::contracts::{ResourceConfig, ResourceKind};
    let name = request.name.trim();
    if name.is_empty() || name.len() > 80 {
        return Err(AuthStackError::validation("resource name is invalid"));
    }
    match &request.config {
        ResourceConfig::Rest { base_url, .. } => {
            validate_http_url(base_url, allow_private)?;
        }
        ResourceConfig::Postgres {
            host,
            port,
            database,
            user,
            password_secret_id,
            ..
        } => {
            if host.trim() == "@app" {
                return Err(AuthStackError::validation(
                    "the app database is reserved for internal storage and cannot be used as a dashboard connector",
                ));
            } else if host.trim().is_empty()
                || *port == 0
                || database.trim().is_empty()
                || user.trim().is_empty()
            {
                return Err(AuthStackError::validation(
                    "postgres host, port, database, and user are required",
                ));
            } else if password_secret_id.trim().is_empty() {
                return Err(AuthStackError::validation(
                    "postgres password secret is required",
                ));
            }
        }
        ResourceConfig::Grpc {
            host,
            port,
            gateway_base_url,
            ..
        } => {
            let has_gateway = gateway_base_url
                .as_ref()
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            if !has_gateway && (host.trim().is_empty() || *port == 0) {
                return Err(AuthStackError::validation(
                    "grpc requires host/port or a gateway_base_url",
                ));
            }
            if has_gateway {
                if let Some(url) = gateway_base_url {
                    validate_http_url(url, allow_private)?;
                }
            }
        }
        ResourceConfig::Builtin => {}
    }
    let id = request
        .id
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| new_id("res"));
    let resource = crate::contracts::DashboardResource {
        id: id.clone(),
        name: name.to_owned(),
        kind: request.kind,
        auth: request.auth,
        default_headers: request.default_headers,
        config: request.config,
    };
    // kind must match config tag
    let kind_ok = matches!(
        (&resource.kind, &resource.config),
        (ResourceKind::Builtin, ResourceConfig::Builtin)
            | (ResourceKind::Rest, ResourceConfig::Rest { .. })
            | (ResourceKind::Postgres, ResourceConfig::Postgres { .. })
            | (ResourceKind::Grpc, ResourceConfig::Grpc { .. })
    );
    if !kind_ok {
        return Err(AuthStackError::validation(
            "resource kind does not match config",
        ));
    }
    let mut resources = load_resources(org_id).await?;
    if let Some(slot) = resources.iter_mut().find(|r| r.id == id) {
        *slot = resource.clone();
    } else {
        if resources.len() >= MAX_RESOURCES {
            return Err(AuthStackError::validation("too many resources"));
        }
        resources.push(resource.clone());
    }
    save_resources(org_id, &resources).await?;
    Ok(resource_to_summary(&resource))
}

pub async fn upsert_query(
    org_id: &str,
    request: crate::contracts::QueryUpsert,
) -> AuthStackResult<crate::contracts::QuerySummary> {
    let name = request.name.trim();
    if name.is_empty() || name.len() > 80 {
        return Err(AuthStackError::validation("query name is invalid"));
    }
    let resources = load_resources(org_id).await?;
    if !resources.iter().any(|r| r.id == request.resource_id) {
        return Err(AuthStackError::not_found("resource not found"));
    }
    let id = request
        .id
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| new_id("qry"));
    let query = crate::contracts::DashboardQuery {
        id: id.clone(),
        name: name.to_owned(),
        resource_id: request.resource_id,
        transform: request.transform,
        config: request.config,
    };
    let mut queries = load_queries(org_id).await?;
    if let Some(slot) = queries.iter_mut().find(|q| q.id == id) {
        *slot = query.clone();
    } else {
        if queries.len() >= MAX_QUERIES {
            return Err(AuthStackError::validation("too many queries"));
        }
        queries.push(query.clone());
    }
    save_queries(org_id, &queries).await?;
    Ok(query_to_summary(&query, &resources))
}

pub async fn delete_resource(org_id: &str, resource_id: &str) -> AuthStackResult<()> {
    let mut resources = load_resources(org_id).await?;
    let before = resources.len();
    resources.retain(|r| r.id != resource_id);
    if resources.len() == before {
        return Err(AuthStackError::not_found("resource not found"));
    }
    // Drop queries bound to this resource.
    let mut queries = load_queries(org_id).await?;
    queries.retain(|q| q.resource_id != resource_id);
    save_resources(org_id, &resources).await?;
    save_queries(org_id, &queries).await
}

pub async fn delete_query(org_id: &str, query_id: &str) -> AuthStackResult<()> {
    let mut queries = load_queries(org_id).await?;
    let before = queries.len();
    queries.retain(|q| q.id != query_id);
    if queries.len() == before {
        return Err(AuthStackError::not_found("query not found"));
    }
    save_queries(org_id, &queries).await
}

pub async fn delete_data_source(user_id: &str, source_id: &str) -> AuthStackResult<()> {
    let mut sources = load_data_sources(user_id).await?;
    let before = sources.len();
    sources.retain(|s| s.id != source_id);
    if sources.len() == before {
        return Err(AuthStackError::not_found("source not found"));
    }
    save_data_sources(user_id, &sources).await?;
    Ok(())
}
