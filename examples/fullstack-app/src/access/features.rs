//! Feature registries: nav, widgets, and high-visibility actions.

use super::context::AccessContext;
use super::permission::PermissionId;
use super::requirement::AccessRequirement;
use crate::contracts::{BoardNode, DashboardWidgetKind};

// ── Nav ────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavSection {
    Product,
    Settings,
    System,
}

#[derive(Clone, Debug)]
pub enum NavHref {
    Static(&'static str),
    /// Path segment after `/org/{slug}/settings/` (e.g. `general`).
    SettingsSection(&'static str),
}

#[derive(Clone, Debug)]
pub struct NavItem {
    pub id: &'static str,
    pub label: &'static str,
    pub icon: Option<&'static str>,
    pub section: NavSection,
    pub requirement: AccessRequirement,
    pub href: NavHref,
}

const SETTINGS_GENERAL: &[PermissionId] = &[PermissionId::ORGANIZATION_VIEW];
const SETTINGS_MEMBERS: &[PermissionId] = &[PermissionId::MEMBER_VIEW];
const SETTINGS_ROLES: &[PermissionId] = &[PermissionId::ROLE_VIEW];
const SETTINGS_AUDIT: &[PermissionId] = &[PermissionId::AUDIT_VIEW];

#[must_use]
pub fn nav_product_items() -> &'static [NavItem] {
    &[
        NavItem {
            id: "overview",
            label: "Overview",
            icon: Some("overview"),
            section: NavSection::Product,
            requirement: AccessRequirement::Authenticated,
            href: NavHref::Static("/dashboard"),
        },
        NavItem {
            id: "organizations",
            label: "Organizations",
            icon: Some("organizations"),
            section: NavSection::Product,
            requirement: AccessRequirement::Authenticated,
            href: NavHref::Static("/organizations"),
        },
    ]
}

#[must_use]
pub fn nav_settings_items() -> &'static [NavItem] {
    &[
        NavItem {
            id: "settings-general",
            label: "General",
            icon: None,
            section: NavSection::Settings,
            requirement: AccessRequirement::AllPermissions(SETTINGS_GENERAL),
            href: NavHref::SettingsSection("general"),
        },
        NavItem {
            id: "settings-members",
            label: "Members",
            icon: None,
            section: NavSection::Settings,
            requirement: AccessRequirement::AllPermissions(SETTINGS_MEMBERS),
            href: NavHref::SettingsSection("members"),
        },
        NavItem {
            id: "settings-invitations",
            label: "Invitations",
            icon: None,
            section: NavSection::Settings,
            requirement: AccessRequirement::AllPermissions(SETTINGS_MEMBERS),
            href: NavHref::SettingsSection("invitations"),
        },
        NavItem {
            id: "settings-roles",
            label: "Roles",
            icon: None,
            section: NavSection::Settings,
            requirement: AccessRequirement::AllPermissions(SETTINGS_ROLES),
            href: NavHref::SettingsSection("roles"),
        },
        NavItem {
            id: "settings-audit",
            label: "Audit log",
            icon: None,
            section: NavSection::Settings,
            requirement: AccessRequirement::AllPermissions(SETTINGS_AUDIT),
            href: NavHref::SettingsSection("audit"),
        },
        NavItem {
            id: "settings-danger",
            label: "Danger zone",
            icon: None,
            section: NavSection::Settings,
            requirement: AccessRequirement::AllPermissions(SETTINGS_GENERAL),
            href: NavHref::SettingsSection("danger"),
        },
    ]
}

/// True if the user may open any settings section (for org-switcher link).
#[must_use]
pub fn can_view_any_settings(ctx: &super::context::AccessContext) -> bool {
    nav_settings_items()
        .iter()
        .any(|item| item.requirement.is_satisfied_by(ctx))
}

// ── Widgets ────────────────────────────────────────────────────────────────

const DASHBOARD_VIEW: &[PermissionId] = &[PermissionId::DASHBOARD_VIEW];
const DASHBOARD_MANAGE: &[PermissionId] = &[PermissionId::DASHBOARD_MANAGE];
const QUERY_VIEW: &[PermissionId] = &[PermissionId::QUERY_VIEW];
const AUDIT_VIEW: &[PermissionId] = &[PermissionId::AUDIT_VIEW];
const QUERY_AND_MANAGE: &[PermissionId] =
    &[PermissionId::DASHBOARD_MANAGE, PermissionId::QUERY_VIEW];

#[must_use]
pub fn widget_view_requirement(kind: DashboardWidgetKind) -> AccessRequirement {
    match kind {
        DashboardWidgetKind::Activity => AccessRequirement::AllPermissions(AUDIT_VIEW),
        DashboardWidgetKind::HttpPanel
        | DashboardWidgetKind::BoundMetric
        | DashboardWidgetKind::BoundList
        | DashboardWidgetKind::BoundTable => AccessRequirement::AllPermissions(QUERY_VIEW),
        _ => AccessRequirement::AllPermissions(DASHBOARD_VIEW),
    }
}

#[must_use]
pub fn widget_manage_requirement(kind: DashboardWidgetKind) -> AccessRequirement {
    match kind {
        DashboardWidgetKind::HttpPanel
        | DashboardWidgetKind::BoundMetric
        | DashboardWidgetKind::BoundList
        | DashboardWidgetKind::BoundTable => AccessRequirement::AllPermissions(QUERY_AND_MANAGE),
        _ => AccessRequirement::AllPermissions(DASHBOARD_MANAGE),
    }
}

// ── Actions ────────────────────────────────────────────────────────────────

const SEED_DEMOS: &[PermissionId] = &[
    PermissionId::RESOURCE_MANAGE,
    PermissionId::QUERY_MANAGE,
];
const VAULT_MANAGE: &[PermissionId] = &[PermissionId::VAULT_MANAGE];

#[must_use]
pub fn action_seed_demos() -> AccessRequirement {
    // Backend also requires AAL2 in production via mutation_step_up; UI hides when
    // permissions missing. Assurance is enforced server-side.
    AccessRequirement::AllPermissions(SEED_DEMOS)
}

#[must_use]
pub fn action_vault_create_secret() -> AccessRequirement {
    AccessRequirement::AllPermissions(VAULT_MANAGE)
}

/// Drop widgets (and empty containers) the viewer cannot see.
#[must_use]
pub fn filter_board_nodes(nodes: Vec<BoardNode>, ctx: &AccessContext) -> Vec<BoardNode> {
    nodes
        .into_iter()
        .filter_map(|node| match node {
            BoardNode::Container {
                id,
                kind,
                col_span,
                children,
            } => {
                let children = filter_board_nodes(children, ctx);
                if children.is_empty() {
                    None
                } else {
                    Some(BoardNode::Container {
                        id,
                        kind,
                        col_span,
                        children,
                    })
                }
            }
            widget if is_hidden_from(&widget, ctx) => None,
            widget => Some(widget),
        })
        .collect()
}

/// True when the viewer is not allowed to see this node.
///
/// Containers are structural and always visible; only widgets carry a view
/// requirement.
fn is_hidden_from(node: &BoardNode, ctx: &AccessContext) -> bool {
    match node {
        BoardNode::Widget { kind, .. } => {
            !widget_view_requirement(kind.clone()).is_satisfied_by(ctx)
        }
        BoardNode::Container { .. } => false,
    }
}

fn collect_node_ids(nodes: &[BoardNode], out: &mut std::collections::HashSet<String>) {
    for node in nodes {
        out.insert(node.id().to_owned());
        if let BoardNode::Container { children, .. } = node {
            collect_node_ids(children, out);
        }
    }
}

fn collect_hidden_nodes(
    nodes: &[BoardNode],
    ctx: &AccessContext,
    present: &std::collections::HashSet<String>,
    out: &mut Vec<BoardNode>,
) {
    for node in nodes {
        if present.contains(node.id()) {
            continue;
        }
        if is_hidden_from(node, ctx) {
            out.push(node.clone());
        } else if let BoardNode::Container { children, .. } = node {
            collect_hidden_nodes(children, ctx, present, out);
        }
    }
}

fn stored_children<'a>(nodes: &'a [BoardNode], container_id: &str) -> Option<&'a [BoardNode]> {
    for node in nodes {
        if let BoardNode::Container { id, children, .. } = node {
            if id == container_id {
                return Some(children);
            }
            if let Some(found) = stored_children(children, container_id) {
                return Some(found);
            }
        }
    }
    None
}

fn merge_hidden_level(
    stored_root: &[BoardNode],
    stored_level: &[BoardNode],
    incoming: Vec<BoardNode>,
    ctx: &AccessContext,
    present: &std::collections::HashSet<String>,
    orphans: &mut Vec<BoardNode>,
) -> Vec<BoardNode> {
    // Containers the caller kept are matched by id against the stored tree, so a
    // container that moved to a new parent still merges against its own children.
    let mut merged: Vec<BoardNode> = incoming
        .into_iter()
        .map(|node| match node {
            BoardNode::Container {
                id,
                kind,
                col_span,
                children,
            } => {
                let children = match stored_children(stored_root, &id) {
                    Some(stored_children) => merge_hidden_level(
                        stored_root,
                        stored_children,
                        children,
                        ctx,
                        present,
                        orphans,
                    ),
                    None => children,
                };
                BoardNode::Container {
                    id,
                    kind,
                    col_span,
                    children,
                }
            }
            widget => widget,
        })
        .collect();

    for (index, node) in stored_level.iter().enumerate() {
        if present.contains(node.id()) {
            continue;
        }
        if is_hidden_from(node, ctx) {
            merged.insert(index.min(merged.len()), node.clone());
        } else if let BoardNode::Container { children, .. } = node {
            // The caller removed a container they could see; anything inside it
            // they could not see is rescued to the board root instead of lost.
            collect_hidden_nodes(children, ctx, present, orphans);
        }
    }
    merged
}

/// Restore board nodes the caller was never allowed to see into an incoming tree.
///
/// The board is rendered permission-filtered, so a save carries only the nodes
/// the caller can see. Writing that tree verbatim would delete other members'
/// widgets. Hidden stored nodes missing from `incoming` are put back at their
/// stored position; hidden nodes whose parent container the caller deleted are
/// appended at the root so they survive the write.
#[must_use]
pub fn merge_hidden_board_nodes(
    stored: Vec<BoardNode>,
    incoming: Vec<BoardNode>,
    ctx: &AccessContext,
) -> Vec<BoardNode> {
    let mut present = std::collections::HashSet::new();
    collect_node_ids(&incoming, &mut present);
    let mut orphans = Vec::new();
    let mut merged = merge_hidden_level(&stored, &stored, incoming, ctx, &present, &mut orphans);
    merged.append(&mut orphans);
    merged
}

/// Widget kinds the user may add from the picker.
#[must_use]
pub fn filter_widget_catalog<'a>(
    kinds: impl IntoIterator<Item = &'a DashboardWidgetKind>,
    ctx: &AccessContext,
) -> Vec<DashboardWidgetKind> {
    kinds
        .into_iter()
        .filter(|kind| widget_manage_requirement((*kind).clone()).is_satisfied_by(ctx))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::context::{AccessContext, AssuranceLevel, PermissionSet};

    fn ctx(perms: &[&str]) -> AccessContext {
        AccessContext {
            authenticated: true,
            permissions: PermissionSet::from_iter(perms.iter().copied()),
            assurance: AssuranceLevel::Aal1,
            system_administrator: false,
        }
    }

    #[test]
    fn settings_nav_filters_by_permission() {
        let limited = ctx(&["organization.view", "member.view"]);
        let visible: Vec<_> = nav_settings_items()
            .iter()
            .filter(|i| i.requirement.is_satisfied_by(&limited))
            .map(|i| i.id)
            .collect();
        assert!(visible.contains(&"settings-general"));
        assert!(visible.contains(&"settings-members"));
        assert!(!visible.contains(&"settings-roles"));
        assert!(!visible.contains(&"settings-audit"));
    }

    #[test]
    fn query_widgets_require_query_view() {
        let no_query = ctx(&["dashboard.view"]);
        assert!(
            !widget_view_requirement(DashboardWidgetKind::BoundTable).is_satisfied_by(&no_query)
        );
        let with_query = ctx(&["query.view"]);
        assert!(
            widget_view_requirement(DashboardWidgetKind::BoundTable).is_satisfied_by(&with_query)
        );
    }

    fn widget(id: &str, kind: DashboardWidgetKind) -> BoardNode {
        BoardNode::Widget {
            id: id.to_owned(),
            kind,
            col_span: 6,
            note_text: None,
            source_id: None,
            bind: crate::contracts::WidgetBind::default(),
            http_mode: crate::contracts::HttpDisplayMode::List,
        }
    }

    fn row(id: &str, children: Vec<BoardNode>) -> BoardNode {
        BoardNode::Container {
            id: id.to_owned(),
            kind: crate::contracts::BoardContainerKind::Row,
            col_span: 12,
            children,
        }
    }

    fn ids(nodes: &[BoardNode]) -> Vec<String> {
        nodes.iter().map(|node| node.id().to_owned()).collect()
    }

    /// The exact #85 data-loss case: a member without `query.view` saves the
    /// board they can see, which omits every bound widget.
    #[test]
    fn merge_restores_widgets_the_caller_could_not_see() {
        let viewer = ctx(&["dashboard.view"]);
        let stored = vec![
            widget("w-notes", DashboardWidgetKind::Notes),
            widget("w-bound", DashboardWidgetKind::BoundTable),
            widget("w-sessions", DashboardWidgetKind::Sessions),
        ];
        let incoming = filter_board_nodes(stored.clone(), &viewer);
        assert_eq!(ids(&incoming), ["w-notes", "w-sessions"]);

        let merged = merge_hidden_board_nodes(stored, incoming, &viewer);
        assert_eq!(ids(&merged), ["w-notes", "w-bound", "w-sessions"]);
    }

    #[test]
    fn merge_restores_hidden_children_inside_kept_containers() {
        let viewer = ctx(&["dashboard.view"]);
        let stored = vec![row(
            "c-row",
            vec![
                widget("w-bound", DashboardWidgetKind::BoundMetric),
                widget("w-notes", DashboardWidgetKind::Notes),
            ],
        )];
        let incoming = filter_board_nodes(stored.clone(), &viewer);

        let merged = merge_hidden_board_nodes(stored, incoming, &viewer);
        match &merged[..] {
            [BoardNode::Container { children, .. }] => {
                assert_eq!(ids(children), ["w-bound", "w-notes"]);
            }
            other => panic!("expected one container, got {other:?}"),
        }
    }

    /// Deleting a container the caller can see must not silently take unseen
    /// widgets with it.
    #[test]
    fn merge_rescues_hidden_nodes_from_a_deleted_container() {
        let viewer = ctx(&["dashboard.view"]);
        let stored = vec![
            row(
                "c-row",
                vec![
                    widget("w-bound", DashboardWidgetKind::BoundList),
                    widget("w-notes", DashboardWidgetKind::Notes),
                ],
            ),
            widget("w-sessions", DashboardWidgetKind::Sessions),
        ];
        let incoming = vec![widget("w-sessions", DashboardWidgetKind::Sessions)];

        let merged = merge_hidden_board_nodes(stored, incoming, &viewer);
        assert_eq!(ids(&merged), ["w-sessions", "w-bound"]);
    }

    /// Removals a caller is entitled to make still take effect.
    #[test]
    fn merge_keeps_deletions_the_caller_was_allowed_to_make() {
        let manager = ctx(&["dashboard.view", "query.view", "audit.view"]);
        let stored = vec![
            widget("w-notes", DashboardWidgetKind::Notes),
            widget("w-bound", DashboardWidgetKind::BoundTable),
        ];
        let incoming = vec![widget("w-notes", DashboardWidgetKind::Notes)];

        let merged = merge_hidden_board_nodes(stored, incoming, &manager);
        assert_eq!(ids(&merged), ["w-notes"]);
    }

    /// Reordering and moving visible tiles round-trips unchanged.
    #[test]
    fn merge_preserves_caller_reordering() {
        let viewer = ctx(&["dashboard.view"]);
        let stored = vec![
            widget("w-notes", DashboardWidgetKind::Notes),
            widget("w-bound", DashboardWidgetKind::BoundMetric),
            widget("w-sessions", DashboardWidgetKind::Sessions),
        ];
        let incoming = vec![
            widget("w-sessions", DashboardWidgetKind::Sessions),
            widget("w-notes", DashboardWidgetKind::Notes),
        ];

        let merged = merge_hidden_board_nodes(stored, incoming, &viewer);
        assert_eq!(ids(&merged), ["w-sessions", "w-bound", "w-notes"]);
    }
}
