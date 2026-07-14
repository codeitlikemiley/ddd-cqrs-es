#![allow(unused_imports)]
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

use super::dashboard::BuiltinSourceKey;

// ─── Retool-style Resource / Query platform ─────────────────────────────────

/// Where an API key is injected.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApiKeyLocation {
    Header,
    QueryParam,
}

impl Default for ApiKeyLocation {
    fn default() -> Self {
        Self::Header
    }
}

/// Secret-capable header / metadata / query-param value.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HeaderValue {
    Literal { value: String },
    Secret { secret_id: String },
}

impl HeaderValue {
    pub fn literal(value: impl Into<String>) -> Self {
        Self::Literal {
            value: value.into(),
        }
    }

    pub fn secret(secret_id: impl Into<String>) -> Self {
        Self::Secret {
            secret_id: secret_id.into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HeaderBag {
    pub name: String,
    pub value: HeaderValue,
}

/// Connection authentication (applied server-side when executing queries).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResourceAuth {
    None,
    Bearer {
        secret_id: String,
    },
    Basic {
        username: String,
        password_secret_id: String,
    },
    ApiKey {
        location: ApiKeyLocation,
        name: String,
        secret_id: String,
    },
    OAuth2ClientCredentials {
        token_url: String,
        client_id: String,
        client_secret_id: String,
        #[serde(default)]
        scopes: Vec<String>,
        #[serde(default)]
        audience: Option<String>,
    },
}

impl Default for ResourceAuth {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Builtin,
    Rest,
    Postgres,
    Grpc,
}

impl ResourceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::Rest => "rest",
            Self::Postgres => "postgres",
            Self::Grpc => "grpc",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Builtin => "App builtins",
            Self::Rest => "REST API",
            Self::Postgres => "PostgreSQL",
            Self::Grpc => "gRPC",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PostgresSslMode {
    Disable,
    Prefer,
    Require,
}

impl Default for PostgresSslMode {
    fn default() -> Self {
        Self::Prefer
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GrpcProtoSource {
    Reflection,
    ProtoFile { content_id: String },
}

impl Default for GrpcProtoSource {
    fn default() -> Self {
        Self::Reflection
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResourceConfig {
    Builtin,
    Rest {
        base_url: String,
        #[serde(default)]
        timeout_ms: u32,
    },
    Postgres {
        host: String,
        port: u16,
        database: String,
        user: String,
        password_secret_id: String,
        #[serde(default)]
        ssl_mode: PostgresSslMode,
    },
    Grpc {
        host: String,
        port: u16,
        #[serde(default)]
        tls: bool,
        #[serde(default)]
        proto_source: GrpcProtoSource,
        #[serde(default = "default_grpc_max_message")]
        max_message_bytes: u32,
        #[serde(default = "default_true")]
        use_proto_json: bool,
        /// When set, unary calls are POSTed as JSON to this base (grpc-gateway / envoy transcoder).
        /// Native Spin HTTP/2 gRPC client is gated; gateway is the supported path today.
        #[serde(default)]
        gateway_base_url: Option<String>,
    },
}

fn default_grpc_max_message() -> u32 {
    4 * 1024 * 1024
}

fn default_true() -> bool {
    true
}

/// Saved connection (Retool "Resource"). Secrets never leave the server.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DashboardResource {
    pub id: String,
    pub name: String,
    pub kind: ResourceKind,
    #[serde(default)]
    pub auth: ResourceAuth,
    #[serde(default)]
    pub default_headers: Vec<HeaderBag>,
    pub config: ResourceConfig,
}

/// Client-safe resource summary (no secret ids resolved, no passwords).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceSummary {
    pub id: String,
    pub name: String,
    pub kind: ResourceKind,
    pub auth_type: String,
    pub detail: String,
    pub header_names: Vec<String>,
    pub has_secrets: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TransformStep {
    JsonPath { path: String },
    AsArray,
    MapFields {
        /// target_field → source_path
        #[serde(default)]
        fields: Vec<(String, String)>,
    },
    Limit { n: u32 },
    PickScalar { path: String },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl HttpMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "GET" => Some(Self::Get),
            "POST" => Some(Self::Post),
            "PUT" => Some(Self::Put),
            "PATCH" => Some(Self::Patch),
            "DELETE" => Some(Self::Delete),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryParam {
    pub name: String,
    pub value: HeaderValue,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QueryConfig {
    Rest {
        method: HttpMethod,
        path: String,
        #[serde(default)]
        query_params: Vec<QueryParam>,
        #[serde(default)]
        headers: Vec<HeaderBag>,
        #[serde(default)]
        body: Option<String>,
    },
    Postgres {
        sql: String,
    },
    Grpc {
        service: String,
        method: String,
        #[serde(default)]
        request_json: String,
        #[serde(default)]
        headers: Vec<HeaderBag>,
    },
    Builtin {
        key: BuiltinSourceKey,
    },
}

/// Executable query against a resource (Retool "Resource query").
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DashboardQuery {
    pub id: String,
    pub name: String,
    pub resource_id: String,
    #[serde(default)]
    pub transform: Vec<TransformStep>,
    pub config: QueryConfig,
}

/// Client-safe query summary.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuerySummary {
    pub id: String,
    pub name: String,
    pub resource_id: String,
    pub resource_kind: ResourceKind,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryMeta {
    pub resource_kind: ResourceKind,
    #[serde(default)]
    pub status: Option<u16>,
    #[serde(default)]
    pub grpc_status: Option<i32>,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default)]
    pub row_count: Option<u32>,
    #[serde(default)]
    pub truncated: bool,
}

/// Safe query execution result (never includes secrets).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryResult {
    pub query_id: String,
    pub ok: bool,
    #[serde(default)]
    pub error: Option<String>,
    /// Payload before transform pipeline.
    pub raw_json: String,
    /// Payload after transform pipeline.
    pub data_json: String,
    pub meta: QueryMeta,
}

impl QueryResult {
    pub fn err(query_id: impl Into<String>, kind: ResourceKind, message: impl Into<String>) -> Self {
        Self {
            query_id: query_id.into(),
            ok: false,
            error: Some(message.into()),
            raw_json: "null".to_owned(),
            data_json: "null".to_owned(),
            meta: QueryMeta {
                resource_kind: kind,
                status: None,
                grpc_status: None,
                duration_ms: 0,
                row_count: None,
                truncated: false,
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceUpsert {
    pub id: Option<String>,
    pub name: String,
    pub kind: ResourceKind,
    #[serde(default)]
    pub auth: ResourceAuth,
    #[serde(default)]
    pub default_headers: Vec<HeaderBag>,
    pub config: ResourceConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryUpsert {
    pub id: Option<String>,
    pub name: String,
    pub resource_id: String,
    #[serde(default)]
    pub transform: Vec<TransformStep>,
    pub config: QueryConfig,
}

