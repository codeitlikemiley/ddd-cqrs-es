#![allow(dead_code)]

use serde::{Deserialize, Serialize};

macro_rules! redacted_debug {
    ($type:ident, visible [$($visible:ident),* $(,)?], secret [$($secret:ident),* $(,)?]) => {
        impl std::fmt::Debug for $type {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let mut debug = formatter.debug_struct(stringify!($type));
                $(debug.field(stringify!($visible), &self.$visible);)*
                $(debug.field(stringify!($secret), &"[REDACTED]");)*
                debug.finish()
            }
        }
    };
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthProviderSummary {
    pub provider_id: String,
    pub display_name: String,
    pub login_url: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthCapabilities {
    pub password_enabled: bool,
    pub oauth_enabled: bool,
    pub passkeys_enabled: bool,
    pub providers: Vec<AuthProviderSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionView {
    pub authenticated: bool,
    pub session_id: Option<String>,
    pub tenant_id: Option<String>,
    pub user_id: Option<String>,
    pub primary_email: Option<String>,
    pub expires_at: Option<String>,
    pub permissions: Vec<String>,
    pub assurance: String,
    pub system_administrator: bool,
    pub issued_at_unix_seconds: Option<u64>,
    pub expires_at_unix_seconds: Option<u64>,
}

/// Editable account profile (app-owned; not the wasi-auth principal).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileView {
    pub email: Option<String>,
    pub first_name: String,
    pub last_name: String,
    pub display_name: String,
    pub username: String,
    pub is_public: bool,
    /// Optional data-URL avatar (`data:image/...;base64,...`).
    pub avatar_data_url: Option<String>,
    /// Public profile path when a username is set, e.g. `/u/jane`.
    pub public_path: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileUpdateRequest {
    pub first_name: String,
    pub last_name: String,
    pub display_name: String,
    pub username: String,
    pub is_public: bool,
    /// When `Some`, replace avatar. Empty string clears. `None` leaves unchanged.
    #[serde(default)]
    pub avatar_data_url: Option<String>,
}

/// Public @handle profile (only returned when the owner marked the profile public).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicProfileView {
    pub username: String,
    pub display_name: String,
    pub first_name: String,
    pub last_name: String,
    pub avatar_data_url: Option<String>,
}

/// Builtin presentation presets (also used as catalog entries).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DashboardWidgetKind {
    MetricSession,
    MetricDevices,
    MetricOrgs,
    MetricSecurity,
    Activity,
    Notifications,
    Sessions,
    Organizations,
    SecurityPosture,
    Notes,
    Checklist,
    /// Legacy generic HTTP panel (still works; prefer Bound*).
    HttpPanel,
    /// Query-bound metric (scalar / one value from QueryResult).
    BoundMetric,
    /// Query-bound list of rows.
    BoundList,
    /// Query-bound table.
    BoundTable,
}

impl DashboardWidgetKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MetricSession => "metric_session",
            Self::MetricDevices => "metric_devices",
            Self::MetricOrgs => "metric_orgs",
            Self::MetricSecurity => "metric_security",
            Self::Activity => "activity",
            Self::Notifications => "notifications",
            Self::Sessions => "sessions",
            Self::Organizations => "organizations",
            Self::SecurityPosture => "security_posture",
            Self::Notes => "notes",
            Self::Checklist => "checklist",
            Self::HttpPanel => "http_panel",
            Self::BoundMetric => "bound_metric",
            Self::BoundList => "bound_list",
            Self::BoundTable => "bound_table",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::MetricSession => "Session status",
            Self::MetricDevices => "Active devices",
            Self::MetricOrgs => "Organizations",
            Self::MetricSecurity => "Security score",
            Self::Activity => "Recent activity",
            Self::Notifications => "Notifications",
            Self::Sessions => "Device sessions",
            Self::Organizations => "Your workspaces",
            Self::SecurityPosture => "Security posture",
            Self::Notes => "Personal note",
            Self::Checklist => "Getting started",
            Self::HttpPanel => "HTTP data panel",
            Self::BoundMetric => "Query metric",
            Self::BoundList => "Query list",
            Self::BoundTable => "Query table",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::MetricSession => "Live assurance level for this browser.",
            Self::MetricDevices => "How many sessions are signed in.",
            Self::MetricOrgs => "Tenants you belong to.",
            Self::MetricSecurity => "MFA + session health at a glance.",
            Self::Activity => "Latest tenant audit events.",
            Self::Notifications => "Alerts and product notices.",
            Self::Sessions => "Review and revoke signed-in devices.",
            Self::Organizations => "Switch or open a workspace.",
            Self::SecurityPosture => "MFA, recovery codes, and session risk.",
            Self::Notes => "A private sticky note on your board.",
            Self::Checklist => "Onboarding steps still open.",
            Self::HttpPanel => "Legacy bind to a query (use Query metric/list/table).",
            Self::BoundMetric => "Show one value from a saved query (field-mapped).",
            Self::BoundList => "Show rows from a saved query as a list.",
            Self::BoundTable => "Show rows from a saved query as a table.",
        }
    }

    /// Default width on a 12-column board.
    pub fn default_span(&self) -> u8 {
        match self {
            Self::MetricSession
            | Self::MetricDevices
            | Self::MetricOrgs
            | Self::MetricSecurity
            | Self::BoundMetric => 3,
            Self::Activity
            | Self::Notifications
            | Self::Sessions
            | Self::Organizations
            | Self::SecurityPosture
            | Self::Notes
            | Self::Checklist
            | Self::HttpPanel
            | Self::BoundList
            | Self::BoundTable => 6,
        }
    }

    pub fn allows_multiple(&self) -> bool {
        matches!(
            self,
            Self::Notes | Self::HttpPanel | Self::BoundMetric | Self::BoundList | Self::BoundTable
        )
    }

    pub fn is_query_bound(&self) -> bool {
        matches!(
            self,
            Self::HttpPanel | Self::BoundMetric | Self::BoundList | Self::BoundTable
        )
    }

    pub fn default_display_mode(&self) -> HttpDisplayMode {
        match self {
            Self::BoundMetric => HttpDisplayMode::Metric,
            Self::BoundTable => HttpDisplayMode::Table,
            _ => HttpDisplayMode::List,
        }
    }

    pub fn catalog() -> &'static [DashboardWidgetKind] {
        &[
            Self::MetricSession,
            Self::MetricDevices,
            Self::MetricOrgs,
            Self::MetricSecurity,
            Self::Activity,
            Self::Notifications,
            Self::Sessions,
            Self::Organizations,
            Self::SecurityPosture,
            Self::Notes,
            Self::Checklist,
            Self::BoundMetric,
            Self::BoundList,
            Self::BoundTable,
            Self::HttpPanel,
        ]
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BoardContainerKind {
    Row,
    Stack,
}

impl BoardContainerKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Row => "row",
            Self::Stack => "stack",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Row => "Row container",
            Self::Stack => "Stack container",
        }
    }
}

/// Field map from QueryResult.data_json → widget view model.
/// Paths are simple dotted paths (`stats.count`, `0.name`, empty = root).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct WidgetBind {
    /// Optional path into data before field maps (e.g. `items`).
    #[serde(default)]
    pub items_path: Option<String>,
    /// Metric: path to the primary value (default: root scalar / first row first field).
    #[serde(default)]
    pub value_path: Option<String>,
    /// Metric: path to secondary label under the value.
    #[serde(default)]
    pub label_path: Option<String>,
    /// List/table row: title field path relative to each item.
    #[serde(default)]
    pub title_path: Option<String>,
    /// List row: subtitle field path.
    #[serde(default)]
    pub subtitle_path: Option<String>,
    /// List/metric meta line path.
    #[serde(default)]
    pub meta_path: Option<String>,
    /// Table: explicit columns as (json_key, header_label). Empty = auto from first row keys.
    #[serde(default)]
    pub columns: Vec<(String, String)>,
}

impl WidgetBind {
    pub fn for_display_mode(mode: &HttpDisplayMode) -> Self {
        match mode {
            HttpDisplayMode::Metric => Self {
                value_path: Some(String::new()),
                label_path: None,
                ..Self::default()
            },
            HttpDisplayMode::List => Self {
                title_path: Some("name".into()),
                subtitle_path: Some("id".into()),
                ..Self::default()
            },
            HttpDisplayMode::Table => Self::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HttpDisplayMode {
    Metric,
    List,
    Table,
}

impl Default for HttpDisplayMode {
    fn default() -> Self {
        Self::List
    }
}

/// Layout node: widget tile or container (row/stack).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BoardNode {
    Widget {
        id: String,
        kind: DashboardWidgetKind,
        /// 1–12 columns on the board grid.
        col_span: u8,
        #[serde(default)]
        note_text: Option<String>,
        /// Optional HTTP / custom source id.
        #[serde(default)]
        source_id: Option<String>,
        #[serde(default)]
        bind: WidgetBind,
        #[serde(default)]
        http_mode: HttpDisplayMode,
    },
    Container {
        id: String,
        kind: BoardContainerKind,
        col_span: u8,
        children: Vec<BoardNode>,
    },
}

impl BoardNode {
    pub fn id(&self) -> &str {
        match self {
            Self::Widget { id, .. } | Self::Container { id, .. } => id,
        }
    }

    pub fn col_span(&self) -> u8 {
        match self {
            Self::Widget { col_span, .. } | Self::Container { col_span, .. } => *col_span,
        }
    }

    pub fn set_col_span(&mut self, span: u8) {
        let span = span.clamp(1, 12);
        match self {
            Self::Widget { col_span, .. } | Self::Container { col_span, .. } => *col_span = span,
        }
    }

    pub fn walk_widgets_mut<F: FnMut(&mut BoardNode)>(&mut self, f: &mut F) {
        f(self);
        if let Self::Container { children, .. } = self {
            for child in children.iter_mut() {
                child.walk_widgets_mut(f);
            }
        }
    }

    pub fn count_nodes(&self) -> usize {
        match self {
            Self::Widget { .. } => 1,
            Self::Container { children, .. } => {
                1 + children.iter().map(BoardNode::count_nodes).sum::<usize>()
            }
        }
    }
}

/// Board layout v2 (nodes tree). v1 `widgets` is migrated on load.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DashboardLayout {
    #[serde(default = "dashboard_layout_version")]
    pub version: u8,
    /// Top-level ordered nodes (widgets and containers).
    #[serde(default)]
    pub nodes: Vec<BoardNode>,
    /// Legacy v1 flat list — only present when reading old KV payloads.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub widgets: Vec<LegacyDashboardWidget>,
}

fn dashboard_layout_version() -> u8 {
    2
}

/// v1 wire shape kept for migration only.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LegacyDashboardWidget {
    pub id: String,
    pub kind: DashboardWidgetKind,
    pub col_span: u8,
    #[serde(default)]
    pub note_text: Option<String>,
}

/// Flat widget handle used by some server updates (notes).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DashboardWidget {
    pub id: String,
    pub kind: DashboardWidgetKind,
    pub col_span: u8,
    #[serde(default)]
    pub note_text: Option<String>,
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default)]
    pub bind: WidgetBind,
    #[serde(default)]
    pub http_mode: HttpDisplayMode,
}

impl DashboardLayout {
    pub fn migrate_if_needed(&mut self) {
        if !self.widgets.is_empty() && self.nodes.is_empty() {
            self.nodes = self
                .widgets
                .drain(..)
                .map(|w| {
                    let span = match w.col_span {
                        1 => 3,
                        2 => 6,
                        3 => 9,
                        4 => 12,
                        s if (1..=12).contains(&s) => s,
                        _ => w.kind.default_span(),
                    };
                    BoardNode::Widget {
                        id: w.id,
                        kind: w.kind,
                        col_span: span,
                        note_text: w.note_text,
                        source_id: None,
                        bind: WidgetBind::default(),
                        http_mode: HttpDisplayMode::List,
                    }
                })
                .collect();
            self.version = 2;
        }
        if self.version < 2 {
            self.version = 2;
        }
    }

    pub fn total_nodes(&self) -> usize {
        self.nodes.iter().map(BoardNode::count_nodes).sum()
    }

    pub fn find_widget_mut(&mut self, id: &str) -> Option<&mut BoardNode> {
        fn find<'a>(nodes: &'a mut [BoardNode], id: &str) -> Option<&'a mut BoardNode> {
            for node in nodes.iter_mut() {
                if node.id() == id {
                    return Some(node);
                }
                if let BoardNode::Container { children, .. } = node
                    && let Some(found) = find(children, id)
                {
                    return Some(found);
                }
            }
            None
        }
        find(&mut self.nodes, id)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DashboardNotification {
    pub id: String,
    pub title: String,
    pub body: String,
    pub level: String,
    pub read: bool,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DataSourceKind {
    Builtin,
    Http,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinSourceKey {
    Session,
    Sessions,
    Organizations,
    Audit,
    Mfa,
    Notifications,
    Health,
}

impl BuiltinSourceKey {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Sessions => "sessions",
            Self::Organizations => "organizations",
            Self::Audit => "audit",
            Self::Mfa => "mfa",
            Self::Notifications => "notifications",
            Self::Health => "health",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HttpHeaderConfig {
    pub name: String,
    /// Literal value, or empty when `secret_id` is set.
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub secret_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataSource {
    pub id: String,
    pub name: String,
    pub kind: DataSourceKind,
    #[serde(default)]
    pub builtin_key: Option<BuiltinSourceKey>,
    #[serde(default)]
    pub method: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub headers: Vec<HttpHeaderConfig>,
    #[serde(default)]
    pub body_template: Option<String>,
    /// Dot-path into JSON (e.g. `data.items`).
    #[serde(default)]
    pub json_path: String,
    /// `one` or `list`.
    #[serde(default = "default_shape_list")]
    pub shape: String,
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_seconds: u32,
}

fn default_shape_list() -> String {
    "list".to_owned()
}

fn default_cache_ttl() -> u32 {
    30
}

/// Client-safe source summary (no secret values).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataSourceSummary {
    pub id: String,
    pub name: String,
    pub kind: DataSourceKind,
    pub builtin_key: Option<BuiltinSourceKey>,
    pub method: String,
    pub url: String,
    pub json_path: String,
    pub shape: String,
    pub header_names: Vec<String>,
    pub has_secrets: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataSourceUpsert {
    pub id: Option<String>,
    pub name: String,
    pub method: String,
    pub url: String,
    pub headers: Vec<HttpHeaderConfig>,
    pub body_template: Option<String>,
    pub json_path: String,
    pub shape: String,
    pub cache_ttl_seconds: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretCreateRequest {
    /// Env-like key: `^[A-Z][A-Z0-9_]{1,63}$` (preferred).
    #[serde(default)]
    pub key: String,
    /// Legacy alias for `key` / human label.
    #[serde(default)]
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub description: String,
    /// `user` (default) or `app` (platform features).
    #[serde(default = "default_vault_scope_user")]
    pub scope: String,
}

fn default_vault_scope_user() -> String {
    "user".to_owned()
}

/// Client-safe vault entry — **never** includes the secret value.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretSummary {
    pub id: String,
    /// Env-like key for connectors (`STRIPE_SECRET_KEY`).
    #[serde(default)]
    pub key: String,
    /// Display label.
    #[serde(default)]
    pub label: String,
    /// Backward-compat: same as `key` or `label`.
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_vault_scope_user")]
    pub scope: String,
    #[serde(default)]
    pub created_at_ms: u64,
    #[serde(default)]
    pub updated_at_ms: u64,
    /// Always the masked placeholder for UI.
    #[serde(default = "default_masked_secret")]
    pub masked_value: String,
}

fn default_masked_secret() -> String {
    "••••••••".to_owned()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretRevealResponse {
    pub id: String,
    pub key: String,
    /// Plaintext — only from dedicated reveal endpoint after authz.
    pub value: String,
    /// UI should remask after this many seconds.
    pub reveal_ttl_seconds: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HttpQueryResult {
    pub source_id: String,
    pub ok: bool,
    pub error: Option<String>,
    /// JSON text payload after path extract (object or array).
    pub data_json: String,
    pub display_mode: HttpDisplayMode,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DashboardSnapshot {
    pub greeting_name: String,
    pub email: Option<String>,
    pub assurance: String,
    pub has_tenant: bool,
    pub tenant_label: Option<String>,
    pub system_administrator: bool,
    pub organization_count: u32,
    pub active_session_count: u32,
    pub security_score: u8,
    pub totp_enrolled: bool,
    pub recovery_codes_remaining: u32,
    pub sessions: Vec<AccountSessionSummary>,
    pub organizations: Vec<OrganizationSummary>,
    pub activity: Vec<AuditEventSummary>,
    pub notifications: Vec<DashboardNotification>,
    pub layout: DashboardLayout,
    pub catalog: Vec<DashboardCatalogItem>,
    pub data_sources: Vec<DataSourceSummary>,
    pub secrets: Vec<SecretSummary>,
    pub http_results: Vec<HttpQueryResult>,
    pub http_enabled: bool,
    /// Retool-style resources (connections).
    #[serde(default)]
    pub resources: Vec<ResourceSummary>,
    /// Saved queries bound by widgets.
    #[serde(default)]
    pub queries: Vec<QuerySummary>,
    /// Results for board-referenced queries.
    #[serde(default)]
    pub query_results: Vec<QueryResult>,
    #[serde(default = "default_true")]
    pub postgres_resources_enabled: bool,
    #[serde(default)]
    pub grpc_resources_enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DashboardCatalogItem {
    pub kind: DashboardWidgetKind,
    pub label: String,
    pub description: String,
    pub default_span: u8,
    pub already_added: bool,
    pub allows_multiple: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DashboardLayoutUpdate {
    pub layout: DashboardLayout,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DashboardNotificationDismiss {
    pub notification_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DashboardNoteUpdate {
    pub widget_id: String,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HttpSourceTestRequest {
    pub source_id: String,
}

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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CsrfTokenResponse {
    pub token: String,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OAuthStartResponse {
    pub provider_id: String,
    pub authorization_url: String,
    pub state: String,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OAuthCallbackRequest {
    pub provider_id: String,
    pub code: Option<String>,
    pub state: Option<String>,
    pub redirect_url: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LoginCompletionResponse {
    pub authenticated: bool,
    pub redirect_url: String,
    pub session_id: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_in_seconds: u64,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PasswordResetStartRequest {
    pub email: String,
    pub redirect_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PasswordResetStartResponse {
    pub accepted: bool,
    pub expires_in_seconds: u64,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapturedMailResponse {
    pub message_kind: String,
    pub recipient: String,
    pub subject: String,
    /// Full plain-text body (greeting, CTA URL, security footer).
    pub body_text: String,
    /// Optional HTML multipart body for productized mail.
    #[serde(default)]
    pub body_html: Option<String>,
    /// One-time action URL extracted for capture-mode deep links.
    #[serde(default)]
    pub action_url: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmailVerificationCompleteRequest {
    pub token: String,
    pub redirect_url: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmailVerificationResendRequest {
    pub email: String,
    pub redirect_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AcceptedResponse {
    pub accepted: bool,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PasswordResetCompleteRequest {
    pub token: String,
    pub password: String,
    pub redirect_url: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmailPasswordLoginRequest {
    pub email: String,
    pub password: String,
    pub redirect_url: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmailPasswordRegisterRequest {
    pub email: String,
    pub password: String,
    pub redirect_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PasskeyStartRequest {
    pub email: Option<String>,
    pub redirect_url: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PasskeyStartResponse {
    pub challenge_id: String,
    pub public_key_options_json: String,
    pub redirect_url: String,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PasskeyVerifyRequest {
    pub challenge_id: String,
    pub credential_json: String,
    pub redirect_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LogoutResponse {
    pub redirect_url: String,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenRefreshResponse {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_in_seconds: u64,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenRefreshRequest {
    pub refresh_token: String,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenVerifyRequest {
    pub access_token: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenVerifyResponse {
    pub active: bool,
    pub subject: String,
    pub tenant_id: Option<String>,
    pub session_id: Option<String>,
    pub expires_at: u64,
    pub scopes: Vec<String>,
    #[serde(skip)]
    pub role_ids: Vec<String>,
    #[serde(skip)]
    pub policy_revision: Option<String>,
    pub assurance: String,
    pub system_administrator: bool,
    pub issued_at_unix_seconds: u64,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PasswordChangeRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountSessionSummary {
    pub session_id: String,
    pub organization_id: Option<String>,
    pub assurance: String,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub current: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountSessionListResponse {
    pub sessions: Vec<AccountSessionSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionRevokeRequest {
    pub session_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MfaStatusResponse {
    pub totp_enrolled: bool,
    pub recovery_codes_remaining: u32,
    pub assurance: String,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MfaEnrollStartResponse {
    pub credential_id: String,
    pub secret_base32: String,
    pub provisioning_uri: String,
}

impl std::fmt::Debug for MfaEnrollStartResponse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MfaEnrollStartResponse")
            .field("credential_id", &self.credential_id)
            .field("secret_base32", &"[REDACTED]")
            .field("provisioning_uri", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MfaCodeRequest {
    pub code: String,
}

impl std::fmt::Debug for MfaCodeRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MfaCodeRequest")
            .field("code", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MfaEnrollConfirmResponse {
    pub recovery_codes: Vec<String>,
    pub assurance: String,
}

impl std::fmt::Debug for MfaEnrollConfirmResponse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MfaEnrollConfirmResponse")
            .field("recovery_codes", &"[REDACTED]")
            .field("assurance", &self.assurance)
            .finish()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SigningKeySummary {
    pub kid: String,
    pub alg: String,
    pub status: String,
    pub active: bool,
    pub source: String,
    pub created_at_ms: Option<u64>,
    pub activated_at_ms: Option<u64>,
    pub retired_at_ms: Option<u64>,
    pub revoked_at_ms: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SigningKeyListResponse {
    pub keys: Vec<SigningKeySummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SigningKeyRotateRequest {
    pub kid: String,
    #[serde(default)]
    pub retire_previous: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SigningKeyRotateResponse {
    pub active_kid: String,
    pub previous_kid: Option<String>,
    pub retired_previous: bool,
    pub keys: Vec<SigningKeySummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthorizationCheckRequest {
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organization_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthorizationCheckResponse {
    pub allowed: bool,
    pub reason: String,
    pub policy_revision: String,
    pub consistency_token: Option<String>,
    pub resource_revision: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthorizationBatchCheckRequest {
    pub checks: Vec<AuthorizationCheckRequest>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthorizationBatchCheckResponse {
    pub results: Vec<AuthorizationCheckResponse>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthorizationCapabilitiesResponse {
    pub provider: String,
    pub batch_check: bool,
    pub list_resources: bool,
    pub consistency_tokens: bool,
    pub max_batch_checks: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrganizationSummary {
    pub organization_id: String,
    pub name: String,
    /// Unique URL key for `/org/{slug}/…` (empty if not registered yet).
    #[serde(default)]
    pub slug: String,
    pub status: String,
    pub current_user_role: String,
    pub permissions: Vec<String>,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrganizationListResponse {
    pub organizations: Vec<OrganizationSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrganizationCreateRequest {
    pub name: String,
    /// Unique URL slug (`acme`). Auto-derived from name when empty.
    #[serde(default)]
    pub slug: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrganizationUpdateRequest {
    pub organization_id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrganizationSelectRequest {
    pub organization_id: String,
}

/// Result of copying legacy per-user dashboard KV into a workspace.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceLegacyMigrateReport {
    pub organization_id: String,
    pub dry_run: bool,
    pub board_copied: bool,
    pub secrets_copied: bool,
    pub secret_rows_copied: u32,
    pub secret_rows_skipped_reenter: u32,
    /// Secret keys that still need manual re-entry (ciphertext under old AAD).
    #[serde(default)]
    pub reenter_required_keys: Vec<String>,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceLegacyMigrateRequest {
    pub organization_id: String,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MembershipSummary {
    pub organization_id: String,
    pub user_id: String,
    pub primary_email: String,
    pub role_id: String,
    pub status: String,
    pub joined_at_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MembershipListResponse {
    pub memberships: Vec<MembershipSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MembershipRoleRequest {
    pub organization_id: String,
    pub user_id: String,
    pub role_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MembershipRemoveRequest {
    pub organization_id: String,
    pub user_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InvitationCreateRequest {
    pub organization_id: String,
    pub email: String,
    pub role_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InvitationAcceptRequest {
    pub token: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InvitationSummary {
    pub invitation_id: String,
    pub organization_id: String,
    pub email: String,
    pub role_id: String,
    pub status: String,
    pub expires_at_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InvitationListResponse {
    pub invitations: Vec<InvitationSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoleSummary {
    pub organization_id: String,
    pub role_id: String,
    pub name: String,
    pub built_in: bool,
    pub permissions: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoleListResponse {
    pub roles: Vec<RoleSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoleUpsertRequest {
    pub organization_id: String,
    pub role_id: String,
    pub name: String,
    pub permissions: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PermissionCatalogResponse {
    pub permissions: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdminUserSummary {
    pub user_id: String,
    pub primary_email: String,
    pub disabled: bool,
    pub email_verified: bool,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdminUserListResponse {
    pub users: Vec<AdminUserSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdminUserStatusRequest {
    pub user_id: String,
    pub disabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdminProviderRequest {
    pub provider_id: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyVersionSummary {
    pub version_id: String,
    pub status: String,
    pub policy_hash: String,
    pub published_by: String,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyVersionListResponse {
    pub versions: Vec<PolicyVersionSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyPublishRequest {
    pub policy_text: String,
    pub schema_text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HealthStatusResponse {
    pub status: String,
    pub storage_backend: String,
    pub mail_transport: String,
    pub authorization_provider: String,
    pub production_mode: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditEventSummary {
    pub sequence: u64,
    pub organization_id: Option<String>,
    pub actor_user_id: String,
    pub action: String,
    pub target_type: String,
    pub target_id: String,
    pub outcome: String,
    pub recorded_at_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditEventListResponse {
    pub events: Vec<AuditEventSummary>,
    pub next_cursor: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageEventTypeCount {
    pub event_type: String,
    pub count: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageProjectionCheckpoint {
    pub projection_name: String,
    pub last_sequence: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageStatusResponse {
    pub event_count: u64,
    pub latest_sequence: u64,
    pub event_types: Vec<StorageEventTypeCount>,
    pub checkpoints: Vec<StorageProjectionCheckpoint>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageProjectionRunResponse {
    pub projection_name: String,
    pub last_sequence_before: u64,
    pub last_sequence_after: u64,
    pub events_scanned: u64,
    pub events_applied: u64,
    pub events_skipped: u64,
}

redacted_debug!(OAuthStartResponse, visible [provider_id], secret [authorization_url, state]);
redacted_debug!(OAuthCallbackRequest, visible [provider_id, redirect_url], secret [code, state]);
redacted_debug!(LoginCompletionResponse, visible [authenticated, redirect_url, expires_in_seconds], secret [session_id, access_token, refresh_token]);
redacted_debug!(PasswordResetStartRequest, visible [redirect_url], secret [email]);
redacted_debug!(CapturedMailResponse, visible [message_kind, subject], secret [recipient, body_text, body_html, action_url]);
redacted_debug!(EmailVerificationCompleteRequest, visible [redirect_url], secret [token]);
redacted_debug!(EmailVerificationResendRequest, visible [redirect_url], secret [email]);
redacted_debug!(PasswordResetCompleteRequest, visible [redirect_url], secret [token, password]);
redacted_debug!(EmailPasswordLoginRequest, visible [redirect_url], secret [email, password]);
redacted_debug!(EmailPasswordRegisterRequest, visible [redirect_url], secret [email, password]);
redacted_debug!(PasskeyStartResponse, visible [redirect_url], secret [challenge_id, public_key_options_json]);
redacted_debug!(PasskeyVerifyRequest, visible [redirect_url], secret [challenge_id, credential_json]);
redacted_debug!(TokenRefreshResponse, visible [expires_in_seconds], secret [access_token, refresh_token]);
redacted_debug!(TokenRefreshRequest, visible [], secret [refresh_token]);
redacted_debug!(TokenVerifyRequest, visible [], secret [access_token]);
redacted_debug!(PasswordChangeRequest, visible [], secret [current_password, new_password]);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mfa_enrollment_and_code_debug_output_is_redacted() {
        let enrollment = MfaEnrollStartResponse {
            credential_id: "totp-one".to_owned(),
            secret_base32: "TOPSECRETBASE32".to_owned(),
            provisioning_uri: "otpauth://secret".to_owned(),
        };
        let confirmation = MfaEnrollConfirmResponse {
            recovery_codes: vec!["AAAA-BBBB-CCCC-DDDD".to_owned()],
            assurance: "aal2".to_owned(),
        };
        let request = MfaCodeRequest {
            code: "123456".to_owned(),
        };

        let debug = format!("{enrollment:?} {confirmation:?} {request:?}");
        assert!(!debug.contains("TOPSECRETBASE32"));
        assert!(!debug.contains("otpauth://secret"));
        assert!(!debug.contains("AAAA-BBBB"));
        assert!(!debug.contains("123456"));
        assert!(debug.contains("[REDACTED]"));
    }

    #[test]
    fn password_and_token_contract_debug_output_is_redacted() {
        let login = EmailPasswordLoginRequest {
            email: "person@example.com".to_owned(),
            password: "correct horse battery staple".to_owned(),
            redirect_url: Some("/organizations".to_owned()),
        };
        let reset = PasswordResetCompleteRequest {
            token: "one-time-reset-token".to_owned(),
            password: "another correct password".to_owned(),
            redirect_url: None,
        };

        let debug = format!("{login:?} {reset:?}");

        assert!(!debug.contains("person@example.com"));
        assert!(!debug.contains("correct horse"));
        assert!(!debug.contains("one-time-reset-token"));
        assert!(debug.contains("[REDACTED]"));
    }
}
