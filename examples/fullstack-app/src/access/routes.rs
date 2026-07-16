//! UI path → permission mapping (shared by server middleware and nav).

use super::permission::PermissionId;

/// Permission required to open a protected UI path, if any.
///
/// Returns `None` when the path is not permission-gated beyond authentication.
#[must_use]
pub fn permission_for_ui_path(path: &str) -> Option<PermissionId> {
    let path = path.trim_end_matches('/');
    if let Some(perm) = workspace_settings_route_permission(path) {
        return Some(perm);
    }
    match path {
        "/admin/auth/signing-keys" => Some(PermissionId::AUTH_SIGNING_KEY_ADMIN),
        "/admin/auth/providers" => Some(PermissionId::AUTH_PROVIDER_WRITE),
        "/admin/auth/redirects" => Some(PermissionId::AUTH_REDIRECT_WRITE),
        "/admin/authorization/policy" => Some(PermissionId::AUTHZ_CHECK),
        "/organizations/settings" => Some(PermissionId::ORGANIZATION_VIEW),
        "/organizations/members" | "/organizations/invitations" => Some(PermissionId::MEMBER_VIEW),
        "/organizations/roles" | "/organizations/permissions" => Some(PermissionId::ROLE_VIEW),
        "/organizations/audit" => Some(PermissionId::AUDIT_VIEW),
        "/admin/users" => Some(PermissionId::SYSTEM_USER_MANAGE),
        "/admin/health" => Some(PermissionId::SYSTEM_HEALTH_READ),
        "/admin/policies" => Some(PermissionId::SYSTEM_POLICY_MANAGE),
        _ => None,
    }
}

/// Map settings path segment (after `/settings/`) to a permission.
#[must_use]
pub fn settings_permission_for_section(section: &str) -> PermissionId {
    match section.trim_matches('/') {
        "" | "general" => PermissionId::ORGANIZATION_VIEW,
        "members" | "invitations" => PermissionId::MEMBER_VIEW,
        "roles" => PermissionId::ROLE_VIEW,
        "audit" => PermissionId::AUDIT_VIEW,
        "danger" => PermissionId::ORGANIZATION_VIEW,
        _ => PermissionId::ORGANIZATION_VIEW,
    }
}

fn workspace_settings_route_permission(path: &str) -> Option<PermissionId> {
    let rest = path.strip_prefix("/org/")?;
    let (_, after_slug) = rest.split_once('/')?;
    let section = if after_slug == "settings" {
        ""
    } else {
        after_slug.strip_prefix("settings/")?
    };
    Some(settings_permission_for_section(section))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_paths_map_to_catalog_permissions() {
        assert_eq!(
            permission_for_ui_path("/org/acme/settings/general"),
            Some(PermissionId::ORGANIZATION_VIEW)
        );
        assert_eq!(
            permission_for_ui_path("/org/acme/settings/members"),
            Some(PermissionId::MEMBER_VIEW)
        );
        assert_eq!(
            permission_for_ui_path("/org/acme/settings/roles"),
            Some(PermissionId::ROLE_VIEW)
        );
        assert_eq!(
            permission_for_ui_path("/org/acme/settings/audit"),
            Some(PermissionId::AUDIT_VIEW)
        );
        assert_eq!(
            permission_for_ui_path("/org/acme/settings/danger"),
            Some(PermissionId::ORGANIZATION_VIEW)
        );
    }

    #[test]
    fn admin_paths_map() {
        assert_eq!(
            permission_for_ui_path("/admin/health"),
            Some(PermissionId::SYSTEM_HEALTH_READ)
        );
        assert_eq!(permission_for_ui_path("/dashboard"), None);
    }
}
