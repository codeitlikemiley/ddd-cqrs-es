#![allow(unused_imports)]
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

use super::admin::AuditEventSummary;
use super::auth::AccountSessionSummary;
use super::organization::OrganizationSummary;
use super::resources::{QueryResult, QuerySummary, ResourceSummary};
use super::vault::SecretSummary;

fn default_true() -> bool {
    true
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

