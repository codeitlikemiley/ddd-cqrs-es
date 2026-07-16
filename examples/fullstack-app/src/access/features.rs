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
            BoardNode::Widget {
                id,
                kind,
                col_span,
                note_text,
                source_id,
                bind,
                http_mode,
            } => {
                if widget_view_requirement(kind.clone()).is_satisfied_by(ctx) {
                    Some(BoardNode::Widget {
                        id,
                        kind,
                        col_span,
                        note_text,
                        source_id,
                        bind,
                        http_mode,
                    })
                } else {
                    None
                }
            }
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
        })
        .collect()
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
}
