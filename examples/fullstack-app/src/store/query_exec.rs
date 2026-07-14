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

/// Execute a saved dashboard query (REST fully supported; other kinds return clear errors).
/// Board data and vault secrets are both scoped to `org_id`.
pub async fn execute_dashboard_query(
    org_id: &str,
    query_id: &str,
    allow_private: bool,
) -> AuthStackResult<crate::contracts::QueryResult> {
    use crate::contracts::{QueryConfig, QueryResult, ResourceConfig, ResourceKind};
    let started = dashboard_now_ms();
    let vault_org_id = Some(org_id);
    let queries = load_queries(org_id).await?;
    let Some(query) = queries.iter().find(|q| q.id == query_id).cloned() else {
        return Ok(QueryResult::err(
            query_id,
            ResourceKind::Rest,
            "query not found",
        ));
    };
    let resources = load_resources(org_id).await?;
    let Some(resource) = resources
        .iter()
        .find(|r| r.id == query.resource_id)
        .cloned()
    else {
        return Ok(QueryResult::err(
            query_id,
            ResourceKind::Rest,
            "resource not found for query",
        ));
    };

    match (&resource.config, &query.config) {
        (ResourceConfig::Rest { base_url, .. }, QueryConfig::Rest { .. }) => {
            execute_rest_query(
                org_id,
                &resource,
                &query,
                base_url,
                allow_private,
                started,
                vault_org_id,
            )
            .await
        }
        (ResourceConfig::Postgres { .. }, QueryConfig::Postgres { sql }) => {
            execute_postgres_dashboard_query(
                org_id,
                &resource,
                &query,
                sql,
                started,
                vault_org_id,
            )
            .await
        }
        (ResourceConfig::Grpc { .. }, QueryConfig::Grpc { .. }) => {
            execute_grpc_dashboard_query(org_id, &resource, &query, allow_private, started).await
        }
        (ResourceConfig::Builtin, QueryConfig::Builtin { .. }) => Ok(QueryResult::err(
            query_id,
            ResourceKind::Builtin,
            "builtin queries are served via the dashboard snapshot, not the query runtime",
        )),
        _ => Ok(QueryResult::err(
            query_id,
            resource.kind,
            "resource kind does not match query kind",
        )),
    }
}

/// Reject non-read SQL (Retool-style SQL mode = read first).
pub fn validate_readonly_sql(sql: &str) -> AuthStackResult<()> {
    let trimmed = sql.trim();
    if trimmed.is_empty() {
        return Err(AuthStackError::validation("SQL is empty"));
    }
    if trimmed.len() > 16_384 {
        return Err(AuthStackError::validation("SQL is too long"));
    }
    // Strip simple line comments for keyword scan.
    let mut cleaned = String::new();
    for line in trimmed.lines() {
        let line = line.split("--").next().unwrap_or("").trim();
        if !line.is_empty() {
            cleaned.push_str(line);
            cleaned.push(' ');
        }
    }
    let upper = cleaned.to_ascii_uppercase();
    // Allow SELECT and WITH … SELECT only.
    let starts_ok = upper.starts_with("SELECT") || upper.starts_with("WITH");
    if !starts_ok {
        return Err(AuthStackError::validation(
            "only SELECT / WITH … SELECT queries are allowed",
        ));
    }
    const BANNED: &[&str] = &[
        " INSERT ",
        " UPDATE ",
        " DELETE ",
        " DROP ",
        " ALTER ",
        " CREATE ",
        " TRUNCATE ",
        " GRANT ",
        " REVOKE ",
        " COPY ",
        " CALL ",
        " DO ",
        " EXECUTE ",
        " VACUUM ",
        " COMMENT ",
        " INTO OUTFILE",
        " INTO DUMPFILE",
    ];
    let padded = format!(" {upper} ");
    for ban in BANNED {
        if padded.contains(ban) {
            return Err(AuthStackError::validation(format!(
                "SQL contains forbidden keyword{}",
                ban.trim()
            )));
        }
    }
    // Multi-statement: ban semicolons except trailing.
    let body = trimmed.trim_end_matches(';').trim();
    if body.contains(';') {
        return Err(AuthStackError::validation(
            "multiple SQL statements are not allowed",
        ));
    }
    Ok(())
}

pub(crate) fn build_postgres_connection_url(
    host: &str,
    port: u16,
    database: &str,
    user: &str,
    password: &str,
    ssl_mode: &crate::contracts::PostgresSslMode,
) -> AuthStackResult<String> {
    use crate::contracts::PostgresSslMode;
    if host.trim().is_empty() || database.trim().is_empty() || user.trim().is_empty() {
        return Err(AuthStackError::validation(
            "postgres host, database, and user are required",
        ));
    }
    // Special host @app uses the app's configured DATABASE_URL / POSTGRES_URL.
    if host.trim() == "@app" {
        return Err(AuthStackError::validation(
            "resolve @app via execute path, not build_postgres_connection_url",
        ));
    }
    let ssl = match ssl_mode {
        PostgresSslMode::Disable => "disable",
        PostgresSslMode::Prefer => "prefer",
        PostgresSslMode::Require => "require",
    };
    // libpq-style URI for Spin pg Connection::open
    let user_enc = form_urlencoded_encode(user);
    let pass_enc = form_urlencoded_encode(password);
    let db_enc = form_urlencoded_encode(database);
    Ok(format!(
        "postgres://{user_enc}:{pass_enc}@{host}:{port}/{db_enc}?sslmode={ssl}"
    ))
}

pub(crate) async fn resolve_postgres_url(
    resource: &crate::contracts::DashboardResource,
    vault_org_id: Option<&str>,
) -> AuthStackResult<String> {
    use crate::contracts::ResourceConfig;
    let ResourceConfig::Postgres {
        host,
        port,
        database,
        user,
        password_secret_id,
        ssl_mode,
    } = &resource.config
    else {
        return Err(AuthStackError::validation("not a postgres resource"));
    };
    if host.trim() == "@app" {
        return database_url("postgres").await;
    }
    let secrets = match vault_org_id.filter(|s| !s.trim().is_empty()) {
        Some(org) => load_secrets_resolved(org).await?,
        None => {
            return Err(AuthStackError::validation(
                "select a workspace to resolve vault secrets for Postgres",
            ));
        }
    };
    let password = secrets
        .iter()
        .find(|s| s.id == *password_secret_id)
        .map(|s| s.value.clone())
        .ok_or_else(|| AuthStackError::validation("postgres password secret missing"))?;
    build_postgres_connection_url(host, *port, database, user, &password, ssl_mode)
}

pub(crate) async fn execute_postgres_dashboard_query(
    user_id: &str,
    resource: &crate::contracts::DashboardResource,
    query: &crate::contracts::DashboardQuery,
    sql: &str,
    started: u64,
    vault_org_id: Option<&str>,
) -> AuthStackResult<crate::contracts::QueryResult> {
    use crate::contracts::{QueryMeta, QueryResult, ResourceKind};
    let _ = user_id;
    if let Err(e) = validate_readonly_sql(sql) {
        return Ok(QueryResult::err(
            &query.id,
            ResourceKind::Postgres,
            e.public_message(),
        ));
    }
    let url = match resolve_postgres_url(resource, vault_org_id).await {
        Ok(u) => u,
        Err(e) => {
            return Ok(QueryResult::err(
                &query.id,
                ResourceKind::Postgres,
                e.public_message(),
            ));
        }
    };

    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        // Cap rows via wrapping subquery when no LIMIT present (best-effort).
        let sql_exec = if sql.to_ascii_uppercase().contains(" LIMIT ") {
            sql.to_owned()
        } else {
            format!("SELECT * FROM ({sql}) AS _dash_q LIMIT 500")
        };
        match ddd_cqrs_es::adapters::execute_spin_pg(&url, &sql_exec, Vec::new()).await {
            Ok(rows) => {
                let truncated = rows.len() >= 500;
                let row_count = rows.len() as u32;
                let raw = Value::Array(rows);
                let raw_json = serde_json::to_string(&raw).unwrap_or_else(|_| "[]".to_owned());
                let transformed = apply_transform_pipeline(raw, &query.transform);
                let data_json =
                    serde_json::to_string(&transformed).unwrap_or_else(|_| "[]".to_owned());
                Ok(QueryResult {
                    query_id: query.id.clone(),
                    ok: true,
                    error: None,
                    raw_json,
                    data_json,
                    meta: QueryMeta {
                        resource_kind: ResourceKind::Postgres,
                        status: None,
                        grpc_status: None,
                        duration_ms: dashboard_now_ms().saturating_sub(started),
                        row_count: Some(row_count),
                        truncated,
                    },
                })
            }
            Err(e) => Ok(QueryResult::err(
                &query.id,
                ResourceKind::Postgres,
                format!("postgres query failed: {e}"),
            )),
        }
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = (url, started, user_id);
        Ok(QueryResult::err(
            &query.id,
            ResourceKind::Postgres,
            "postgres queries require Spin PostgreSQL",
        ))
    }
}

pub(crate) async fn execute_grpc_dashboard_query(
    user_id: &str,
    resource: &crate::contracts::DashboardResource,
    query: &crate::contracts::DashboardQuery,
    allow_private: bool,
    started: u64,
) -> AuthStackResult<crate::contracts::QueryResult> {
    use crate::contracts::{
        HeaderBag, HeaderValue, HttpMethod, QueryConfig, QueryMeta, QueryResult, ResourceConfig,
        ResourceKind,
    };
    let ResourceConfig::Grpc {
        host,
        port,
        tls,
        gateway_base_url,
        use_proto_json,
        ..
    } = &resource.config
    else {
        return Ok(QueryResult::err(
            &query.id,
            ResourceKind::Grpc,
            "not a grpc resource",
        ));
    };
    let QueryConfig::Grpc {
        service,
        method,
        request_json,
        headers: query_headers,
    } = &query.config
    else {
        return Ok(QueryResult::err(
            &query.id,
            ResourceKind::Grpc,
            "not a grpc query",
        ));
    };

    // Supported path: JSON HTTP gateway (grpc-gateway / envoy transcoder).
    let gateway = gateway_base_url
        .as_ref()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());

    if let Some(base) = gateway {
        if !use_proto_json {
            return Ok(QueryResult::err(
                &query.id,
                ResourceKind::Grpc,
                "gateway mode requires use_proto_json=true",
            ));
        }
        // Build a temporary REST-shaped execution using resource auth + metadata headers.
        let path = format!(
            "/{}/{}",
            service.trim_start_matches('/'),
            method.trim_start_matches('/')
        );
        let mut rest_resource = resource.clone();
        rest_resource.config = ResourceConfig::Rest {
            base_url: base,
            timeout_ms: 30_000,
        };
        // Merge default_headers already on resource; add Content-Type.
        let mut headers = rest_resource.default_headers.clone();
        headers.extend(query_headers.iter().cloned());
        if !headers
            .iter()
            .any(|h| h.name.eq_ignore_ascii_case("content-type"))
        {
            headers.push(HeaderBag {
                name: "Content-Type".into(),
                value: HeaderValue::literal("application/json"),
            });
        }
        rest_resource.default_headers = headers;
        let rest_query = crate::contracts::DashboardQuery {
            id: query.id.clone(),
            name: query.name.clone(),
            resource_id: query.resource_id.clone(),
            transform: query.transform.clone(),
            config: QueryConfig::Rest {
                method: HttpMethod::Post,
                path,
                query_params: Vec::new(),
                headers: Vec::new(),
                body: Some(request_json.clone()),
            },
        };
        let base_url = match &rest_resource.config {
            ResourceConfig::Rest { base_url, .. } => base_url.clone(),
            _ => unreachable!(),
        };
        return execute_rest_query(
            user_id,
            &rest_resource,
            &rest_query,
            &base_url,
            allow_private,
            started,
            None,
        )
        .await;
    }

    // Native gRPC is intentionally gated — document Spin HTTP/2 + wasi-grpc for a future cut.
    let _ = (host, port, tls, started);
    Ok(QueryResult::err(
        &query.id,
        ResourceKind::Grpc,
        "native gRPC client is not enabled on this Spin runtime. Set gateway_base_url on the gRPC resource to a grpc-gateway / JSON transcoder URL, or enable AUTH_DASHBOARD_GRPC_ENABLED after upgrading to Spin HTTP/2 outbound.",
    ))
}

pub(crate) async fn execute_legacy_as_query_result(
    user_id: &str,
    source_id: &str,
    allow_private: bool,
    started: u64,
    vault_org_id: Option<&str>,
) -> AuthStackResult<crate::contracts::QueryResult> {
    use crate::contracts::{QueryMeta, QueryResult, ResourceKind};
    match execute_legacy_http_source(user_id, source_id, allow_private, vault_org_id).await {
        Ok(legacy) => Ok(QueryResult {
            query_id: source_id.to_owned(),
            ok: legacy.ok,
            error: legacy.error,
            raw_json: legacy.data_json.clone(),
            data_json: legacy.data_json,
            meta: QueryMeta {
                resource_kind: ResourceKind::Rest,
                status: None,
                grpc_status: None,
                duration_ms: dashboard_now_ms().saturating_sub(started),
                row_count: None,
                truncated: false,
            },
        }),
        Err(e) => Ok(QueryResult::err(
            source_id,
            ResourceKind::Rest,
            e.public_message(),
        )),
    }
}

pub(crate) async fn execute_rest_query(
    user_id: &str,
    resource: &crate::contracts::DashboardResource,
    query: &crate::contracts::DashboardQuery,
    base_url: &str,
    allow_private: bool,
    started: u64,
    vault_org_id: Option<&str>,
) -> AuthStackResult<crate::contracts::QueryResult> {
    use crate::contracts::{
        HttpMethod, QueryConfig, QueryMeta, QueryResult, ResourceAuth, ResourceKind,
    };
    let _ = user_id;
    let QueryConfig::Rest {
        method,
        path,
        query_params,
        headers: query_headers,
        body,
    } = &query.config
    else {
        return Ok(QueryResult::err(
            &query.id,
            ResourceKind::Rest,
            "not a REST query",
        ));
    };

    let secrets = match vault_org_id.filter(|s| !s.trim().is_empty()) {
        Some(org) => load_secrets_resolved(org).await?,
        None => Vec::new(),
    };
    let merged = merge_headers(&resource.default_headers, query_headers);
    let mut resolved_headers: Vec<(String, String)> = Vec::new();
    for h in &merged {
        let name = h.name.trim();
        if name.is_empty() {
            continue;
        }
        resolved_headers.push((name.to_owned(), resolve_header_value(&h.value, &secrets)?));
    }
    let mut resolved_params: Vec<(String, String)> = Vec::new();
    for p in query_params {
        let name = p.name.trim();
        if name.is_empty() {
            continue;
        }
        resolved_params.push((
            name.to_owned(),
            resolve_header_value(&p.value, &secrets)?,
        ));
    }

    // OAuth2 client credentials: fetch token then inject as Bearer if no Authorization yet.
    if let ResourceAuth::OAuth2ClientCredentials {
        token_url,
        client_id,
        client_secret_id,
        scopes,
        audience,
    } = &resource.auth
    {
        let token = fetch_oauth2_client_credentials(
            token_url,
            client_id,
            client_secret_id,
            scopes,
            audience.as_deref(),
            &secrets,
            allow_private,
        )
        .await?;
        if !resolved_headers
            .iter()
            .any(|(n, _)| n.eq_ignore_ascii_case("authorization"))
        {
            resolved_headers.push(("Authorization".to_owned(), format!("Bearer {token}")));
        }
    } else {
        apply_resource_auth(
            &resource.auth,
            &secrets,
            &mut resolved_headers,
            &mut resolved_params,
        )?;
    }

    let mut url = join_base_path(base_url, path);
    if !resolved_params.is_empty() {
        let qs = resolved_params
            .iter()
            .map(|(k, v)| format!("{}={}", form_urlencoded_encode(k), form_urlencoded_encode(v)))
            .collect::<Vec<_>>()
            .join("&");
        if url.contains('?') {
            url.push('&');
        } else {
            url.push('?');
        }
        url.push_str(&qs);
    }
    validate_http_url(&url, allow_private)?;

    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        use http_body_util::BodyExt;
        use spin_sdk::http::{send, FullBody};

        let method_http = match method {
            HttpMethod::Get => http::Method::GET,
            HttpMethod::Post => http::Method::POST,
            HttpMethod::Put => http::Method::PUT,
            HttpMethod::Patch => http::Method::PATCH,
            HttpMethod::Delete => http::Method::DELETE,
        };
        let body_bytes = match method {
            HttpMethod::Get | HttpMethod::Delete => bytes::Bytes::new(),
            _ => bytes::Bytes::from(body.clone().unwrap_or_default().into_bytes()),
        };
        let mut builder = http::Request::builder()
            .method(method_http)
            .uri(&url)
            .header(http::header::ACCEPT, "application/json");
        if matches!(
            method,
            HttpMethod::Post | HttpMethod::Put | HttpMethod::Patch
        ) && !resolved_headers
            .iter()
            .any(|(n, _)| n.eq_ignore_ascii_case("content-type"))
        {
            builder = builder.header(http::header::CONTENT_TYPE, "application/json");
        }
        for (name, value) in &resolved_headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        let request = builder
            .body(FullBody::new(body_bytes))
            .map_err(|e| AuthStackError::transport(format!("build request failed: {e}")))?;
        let response = send(request)
            .await
            .map_err(|e| AuthStackError::transport(format!("HTTP request failed: {e}")))?;
        let status = response.status();
        let status_code = status.as_u16();
        let bytes = response
            .into_body()
            .collect()
            .await
            .map_err(|e| AuthStackError::transport(format!("response body failed: {e:?}")))?
            .to_bytes();
        let duration_ms = dashboard_now_ms().saturating_sub(started);
        let truncated = bytes.len() > MAX_HTTP_RESPONSE_BYTES;
        let slice = if truncated {
            &bytes[..MAX_HTTP_RESPONSE_BYTES]
        } else {
            &bytes
        };
        if truncated {
            return Ok(QueryResult {
                query_id: query.id.clone(),
                ok: false,
                error: Some("response too large".to_owned()),
                raw_json: "null".to_owned(),
                data_json: "null".to_owned(),
                meta: QueryMeta {
                    resource_kind: ResourceKind::Rest,
                    status: Some(status_code),
                    grpc_status: None,
                    duration_ms,
                    row_count: None,
                    truncated: true,
                },
            });
        }
        let parsed: Value = serde_json::from_slice(slice).unwrap_or_else(|_| {
            Value::String(String::from_utf8_lossy(slice).into_owned())
        });
        let raw_json = serde_json::to_string(&parsed).unwrap_or_else(|_| "null".to_owned());
        let transformed = apply_transform_pipeline(parsed, &query.transform);
        let row_count = transformed.as_array().map(|a| a.len() as u32);
        let data_json =
            serde_json::to_string(&transformed).unwrap_or_else(|_| "null".to_owned());
        let ok = status.is_success();
        Ok(QueryResult {
            query_id: query.id.clone(),
            ok,
            error: if ok {
                None
            } else {
                Some(format!("HTTP {status_code}"))
            },
            raw_json,
            data_json,
            meta: QueryMeta {
                resource_kind: ResourceKind::Rest,
                status: Some(status_code),
                grpc_status: None,
                duration_ms,
                row_count,
                truncated: false,
            },
        })
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = (resolved_headers, url, body, method, started, user_id);
        Err(AuthStackError::configuration(
            "HTTP queries require Spin outbound HTTP",
        ))
    }
}

pub(crate) fn join_base_path(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    let path = path.trim();
    if path.is_empty() || path == "/" {
        return base.to_owned();
    }
    if path.starts_with("http://") || path.starts_with("https://") {
        return path.to_owned();
    }
    if path.starts_with('/') {
        format!("{base}{path}")
    } else {
        format!("{base}/{path}")
    }
}

pub(crate) fn form_urlencoded_encode(value: &str) -> String {
    #[cfg(feature = "ssr")]
    {
        form_urlencoded::byte_serialize(value.as_bytes()).collect()
    }
    #[cfg(not(feature = "ssr"))]
    {
        let mut out = String::new();
        for b in value.as_bytes() {
            match *b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    out.push(*b as char);
                }
                b' ' => out.push('+'),
                _ => out.push_str(&format!("%{b:02X}")),
            }
        }
        out
    }
}

pub(crate) fn dashboard_now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub(crate) async fn fetch_oauth2_client_credentials(
    token_url: &str,
    client_id: &str,
    client_secret_id: &str,
    scopes: &[String],
    audience: Option<&str>,
    secrets: &[StoredSecret],
    allow_private: bool,
) -> AuthStackResult<String> {
    validate_http_url(token_url, allow_private)?;
    let client_secret = secrets
        .iter()
        .find(|s| s.id == *client_secret_id)
        .map(|s| s.value.clone())
        .ok_or_else(|| AuthStackError::validation("OAuth client secret missing"))?;
    let mut form = vec![
        ("grant_type".to_owned(), "client_credentials".to_owned()),
        ("client_id".to_owned(), client_id.to_owned()),
        ("client_secret".to_owned(), client_secret),
    ];
    if !scopes.is_empty() {
        form.push(("scope".to_owned(), scopes.join(" ")));
    }
    if let Some(aud) = audience {
        if !aud.is_empty() {
            form.push(("audience".to_owned(), aud.to_owned()));
        }
    }
    let body = form
        .iter()
        .map(|(k, v)| format!("{}={}", form_urlencoded_encode(k), form_urlencoded_encode(v)))
        .collect::<Vec<_>>()
        .join("&");

    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        use http_body_util::BodyExt;
        use spin_sdk::http::{send, FullBody};
        let request = http::Request::builder()
            .method(http::Method::POST)
            .uri(token_url)
            .header(
                http::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .header(http::header::ACCEPT, "application/json")
            .body(FullBody::new(bytes::Bytes::from(body.into_bytes())))
            .map_err(|e| AuthStackError::transport(format!("oauth token build failed: {e}")))?;
        let response = send(request)
            .await
            .map_err(|e| AuthStackError::transport(format!("oauth token request failed: {e}")))?;
        if !response.status().is_success() {
            return Err(AuthStackError::transport(format!(
                "oauth token HTTP {}",
                response.status()
            )));
        }
        let bytes = response
            .into_body()
            .collect()
            .await
            .map_err(|e| AuthStackError::transport(format!("oauth token body failed: {e:?}")))?
            .to_bytes();
        let parsed: Value = serde_json::from_slice(&bytes)
            .map_err(|e| AuthStackError::serialization(format!("oauth token json: {e}")))?;
        parsed
            .get("access_token")
            .and_then(|v| v.as_str())
            .map(|s| s.to_owned())
            .ok_or_else(|| AuthStackError::transport("oauth token response missing access_token"))
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = body;
        Err(AuthStackError::configuration(
            "OAuth token fetch requires Spin outbound HTTP",
        ))
    }
}

pub async fn execute_http_source(
    user_id: &str,
    source_id: &str,
    allow_private: bool,
) -> AuthStackResult<crate::contracts::HttpQueryResult> {
    // Legacy HTTP sources only (org board queries use execute_dashboard_query directly).
    execute_legacy_http_source(user_id, source_id, allow_private, None).await
}

pub(crate) async fn execute_legacy_http_source(
    user_id: &str,
    source_id: &str,
    allow_private: bool,
    vault_org_id: Option<&str>,
) -> AuthStackResult<crate::contracts::HttpQueryResult> {
    let sources = load_data_sources(user_id).await?;
    let source = sources
        .iter()
        .find(|s| s.id == source_id)
        .ok_or_else(|| AuthStackError::not_found("source not found"))?
        .clone();
    if !matches!(source.kind, crate::contracts::DataSourceKind::Http) {
        return Err(AuthStackError::validation("source is not HTTP"));
    }
    validate_http_url(&source.url, allow_private)?;
    let secrets = match vault_org_id.filter(|s| !s.trim().is_empty()) {
        Some(org) => load_secrets_resolved(org).await?,
        None => Vec::new(),
    };
    let mut headers: Vec<(String, String)> = Vec::new();
    for header in &source.headers {
        let name = header.name.trim();
        if name.is_empty() {
            continue;
        }
        let value = if let Some(secret_id) = header.secret_id.as_ref() {
            secrets
                .iter()
                .find(|s| s.id == *secret_id)
                .map(|s| s.value.clone())
                .ok_or_else(|| AuthStackError::validation(format!("missing secret for header {name}")))?
        } else {
            header.value.clone()
        };
        headers.push((name.to_owned(), value));
    }

    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        use http_body_util::BodyExt;
        use spin_sdk::http::{FullBody, send};

        let body = if source.method == "POST" {
            bytes::Bytes::from(
                source
                    .body_template
                    .clone()
                    .unwrap_or_default()
                    .into_bytes(),
            )
        } else {
            bytes::Bytes::new()
        };
        let mut builder = http::Request::builder()
            .method(if source.method == "POST" {
                http::Method::POST
            } else {
                http::Method::GET
            })
            .uri(&source.url)
            .header(http::header::ACCEPT, "application/json");
        for (name, value) in &headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        let request = builder
            .body(FullBody::new(body))
            .map_err(|e| AuthStackError::transport(format!("build request failed: {e}")))?;
        let response = send(request)
            .await
            .map_err(|e| AuthStackError::transport(format!("HTTP request failed: {e}")))?;
        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .map_err(|e| AuthStackError::transport(format!("response body failed: {e:?}")))?
            .to_bytes();
        if bytes.len() > MAX_HTTP_RESPONSE_BYTES {
            return Ok(crate::contracts::HttpQueryResult {
                source_id: source.id,
                ok: false,
                error: Some("response too large".to_owned()),
                data_json: "null".to_owned(),
                display_mode: crate::contracts::HttpDisplayMode::List,
            });
        }
        if !status.is_success() {
            return Ok(crate::contracts::HttpQueryResult {
                source_id: source.id,
                ok: false,
                error: Some(format!("HTTP {status}")),
                data_json: "null".to_owned(),
                display_mode: crate::contracts::HttpDisplayMode::List,
            });
        }
        let parsed: Value = serde_json::from_slice(&bytes).unwrap_or_else(|_| {
            Value::String(String::from_utf8_lossy(&bytes).into_owned())
        });
        let extracted = json_path_get(&parsed, &source.json_path).unwrap_or(parsed);
        let data_json = serde_json::to_string(&extracted).unwrap_or_else(|_| "null".to_owned());
        Ok(crate::contracts::HttpQueryResult {
            source_id: source.id,
            ok: true,
            error: None,
            data_json,
            display_mode: if source.shape == "one" {
                crate::contracts::HttpDisplayMode::Metric
            } else {
                crate::contracts::HttpDisplayMode::List
            },
        })
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = (user_id, headers);
        Err(AuthStackError::configuration(
            "HTTP sources require Spin outbound HTTP",
        ))
    }
}

