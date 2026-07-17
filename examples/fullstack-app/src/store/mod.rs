//! Product storage adapters (KV, SQL, vault, dashboard data).
//! Domain modules re-exported so existing `crate::store::*` call sites keep working.

mod board;
mod health;
mod keys;
mod notifications;
mod org_slug;
mod profile;
mod query_exec;
mod resources;
mod seed;
mod sql;
mod vault;

pub(crate) use board::*;
pub(crate) use health::*;
pub(crate) use keys::*;
pub(crate) use notifications::*;
pub(crate) use org_slug::*;
pub(crate) use profile::*;
pub(crate) use query_exec::*;
pub(crate) use resources::*;
pub(crate) use seed::*;
pub(crate) use sql::*;
pub(crate) use vault::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{
        BoardNode, DashboardLayout, DashboardWidgetKind, HttpDisplayMode, LegacyDashboardWidget,
        WidgetBind,
    };
    use serde_json::json;

    #[test]
    fn postgres_sql_rewrites_indexed_placeholders() {
        assert_eq!(
            postgres_sql("SELECT * FROM auth_users WHERE user_id = ?1 AND status = ?2"),
            "SELECT * FROM auth_users WHERE user_id = $1 AND status = $2"
        );
    }

    #[test]
    fn postgres_sql_rewrites_insert_or_ignore() {
        assert_eq!(
            postgres_sql("INSERT OR IGNORE INTO auth_users (user_id) VALUES (?1)"),
            "INSERT INTO auth_users (user_id) VALUES ($1) ON CONFLICT DO NOTHING"
        );
    }

    #[test]
    fn split_base_and_path_keeps_origin() {
        let (base, path) = super::split_base_and_path("https://api.example.com:8443/v1/items?x=1");
        assert_eq!(base, "https://api.example.com:8443");
        assert_eq!(path, "/v1/items?x=1");
    }

    #[test]
    fn join_base_path_handles_absolute_and_relative() {
        assert_eq!(
            super::join_base_path("https://api.example.com", "/v1/x"),
            "https://api.example.com/v1/x"
        );
        assert_eq!(
            super::join_base_path("https://api.example.com/", "v1/x"),
            "https://api.example.com/v1/x"
        );
        assert_eq!(
            super::join_base_path("https://api.example.com", "https://other.test/a"),
            "https://other.test/a"
        );
    }

    #[test]
    fn validate_readonly_sql_allows_select_blocks_writes() {
        assert!(super::validate_readonly_sql("SELECT 1").is_ok());
        assert!(super::validate_readonly_sql("WITH t AS (SELECT 1) SELECT * FROM t").is_ok());
        assert!(super::validate_readonly_sql("DELETE FROM users").is_err());
        assert!(super::validate_readonly_sql("SELECT 1; DROP TABLE x").is_err());
        assert!(super::validate_readonly_sql("INSERT INTO t VALUES (1)").is_err());
    }

    #[test]
    fn merge_headers_query_wins() {
        use crate::contracts::{HeaderBag, HeaderValue};
        let resource = vec![HeaderBag {
            name: "X-Api-Key".into(),
            value: HeaderValue::literal("from-resource"),
        }];
        let query = vec![HeaderBag {
            name: "x-api-key".into(),
            value: HeaderValue::literal("from-query"),
        }];
        let merged = super::merge_headers(&resource, &query);
        assert_eq!(merged.len(), 1);
        assert!(matches!(
            &merged[0].value,
            HeaderValue::Literal { value } if value == "from-query"
        ));
    }

    #[test]
    fn transform_pipeline_path_and_limit() {
        let raw = serde_json::json!({"data":{"items":[{"n":1},{"n":2},{"n":3}]}});
        let steps = vec![
            crate::contracts::TransformStep::JsonPath {
                path: "data.items".into(),
            },
            crate::contracts::TransformStep::Limit { n: 2 },
        ];
        let out = super::apply_transform_pipeline(raw, &steps);
        assert_eq!(out.as_array().map(|a| a.len()), Some(2));
    }

    #[test]
    fn layout_v1_migrates_to_nodes_with_12_col_spans() {
        let mut layout = DashboardLayout {
            version: 1,
            nodes: Vec::new(),
            widgets: vec![
                LegacyDashboardWidget {
                    id: "a".into(),
                    kind: DashboardWidgetKind::MetricSession,
                    col_span: 1,
                    note_text: None,
                },
                LegacyDashboardWidget {
                    id: "b".into(),
                    kind: DashboardWidgetKind::Activity,
                    col_span: 2,
                    note_text: None,
                },
            ],
        };
        layout.migrate_if_needed();
        assert_eq!(layout.version, 2);
        assert!(layout.widgets.is_empty() || layout.nodes.len() == 2);
        assert_eq!(layout.nodes.len(), 2);
        match &layout.nodes[0] {
            BoardNode::Widget { col_span, .. } => assert_eq!(*col_span, 3),
            _ => panic!("expected widget"),
        }
        match &layout.nodes[1] {
            BoardNode::Widget { col_span, .. } => assert_eq!(*col_span, 6),
            _ => panic!("expected widget"),
        }
    }

    #[test]
    fn json_path_get_reads_nested_and_index() {
        let value = json!({"data": {"items": [{"name": "alpha"}, {"name": "beta"}]}});
        let extracted = json_path_get(&value, "data.items.0.name").unwrap();
        assert_eq!(extracted, json!("alpha"));
        assert_eq!(json_path_get(&value, "").unwrap(), value);
    }

    #[test]
    fn validate_http_url_blocks_private_by_default() {
        assert!(validate_http_url("https://example.com/v1", false).is_ok());
        assert!(validate_http_url("http://127.0.0.1:9/", false).is_err());
        assert!(validate_http_url("http://10.0.0.5/x", false).is_err());
        assert!(validate_http_url("http://169.254.169.254/latest", false).is_err());
        assert!(validate_http_url("http://127.0.0.1:9/", true).is_ok());
        assert!(validate_http_url("ftp://example.com", false).is_err());
    }

    #[test]
    fn vault_secret_key_validation() {
        assert!(validate_vault_secret_key("API_TOKEN").is_ok());
        assert!(validate_vault_secret_key("STRIPE_SECRET_KEY").is_ok());
        assert!(validate_vault_secret_key("A1").is_ok());
        assert!(validate_vault_secret_key("a_token").is_err());
        assert!(validate_vault_secret_key("1TOKEN").is_err());
        assert!(validate_vault_secret_key("HAS-DASH").is_err());
        assert!(validate_vault_secret_key("").is_err());
        assert!(validate_vault_secret_key("X").is_err());
    }

    #[test]
    fn vault_encrypt_decrypt_roundtrip() {
        let key = [7_u8; 32];
        let org = "org-abc";
        let (nonce_b64, ciphertext_b64) =
            encrypt_vault_value(org, "super-secret-value", &key).expect("encrypt");
        let stored = StoredSecret {
            id: "sec1".into(),
            key: "DEMO".into(),
            label: "Demo".into(),
            name: "DEMO".into(),
            description: String::new(),
            scope: "user".into(),
            value: String::new(),
            ciphertext_b64,
            nonce_b64,
            mac_b64: String::new(),
            key_version: "test-v1".into(),
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        let plain = decrypt_vault_value(org, &stored, &key).expect("decrypt");
        assert_eq!(plain, "super-secret-value");
        // Wrong org AAD must fail.
        assert!(decrypt_vault_value("other-org", &stored, &key).is_err());
    }

    #[test]
    fn org_slug_validation_and_suggest() {
        assert!(validate_org_slug("acme").is_ok());
        assert!(validate_org_slug("acme-inc").is_ok());
        assert!(validate_org_slug("a1").is_ok());
        assert!(validate_org_slug("Admin").is_err());
        assert!(validate_org_slug("admin").is_err());
        assert!(validate_org_slug("-acme").is_err());
        assert_eq!(suggest_org_slug("Acme Inc!"), "acme-inc");
    }

    #[test]
    fn default_layout_has_containers_and_twelve_col_metrics() {
        let layout = default_dashboard_layout();
        assert_eq!(layout.version, 2);
        assert!(!layout.nodes.is_empty());
        assert!(layout.total_nodes() >= 10);
        let has_row = layout.nodes.iter().any(|n| {
            matches!(
                n,
                BoardNode::Container {
                    kind: crate::contracts::BoardContainerKind::Row,
                    ..
                }
            )
        });
        assert!(has_row);
    }

    #[test]
    fn board_node_count_includes_nested() {
        let node = BoardNode::Container {
            id: "c".into(),
            kind: crate::contracts::BoardContainerKind::Row,
            col_span: 12,
            children: vec![BoardNode::Widget {
                id: "w".into(),
                kind: DashboardWidgetKind::Notes,
                col_span: 6,
                note_text: Some(String::new()),
                source_id: None,
                bind: WidgetBind::default(),
                http_mode: HttpDisplayMode::List,
            }],
        };
        assert_eq!(node.count_nodes(), 2);
    }
}
