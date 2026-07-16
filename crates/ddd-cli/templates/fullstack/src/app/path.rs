//! Workspace vs public path helpers (layout chrome + topbar titles).

/// Slug-scoped workspace settings (`/org/{slug}/settings` and subpaths).
///
/// These use the settings sidebar shell, not the global workspace rail.
pub(crate) fn is_workspace_settings_path(path: &str) -> bool {
    let path = path.trim_end_matches('/');
    let Some(rest) = path.strip_prefix("/org/") else {
        return false;
    };
    // rest = "{slug}/settings" or "{slug}/settings/…"
    let Some((_, after_slug)) = rest.split_once('/') else {
        return false;
    };
    after_slug == "settings" || after_slug.starts_with("settings/")
}

pub(crate) fn is_workspace_path(path: &str) -> bool {
    let path = path.trim_end_matches('/');
    if path.starts_with("/onboarding") {
        return false;
    }
    // Settings chrome is exclusive — do not wrap with the global workspace rail.
    if is_workspace_settings_path(path) {
        return false;
    }
    path == "/dashboard"
        || path.starts_with("/dashboard/")
        || path.starts_with("/account")
        || path.starts_with("/organizations")
        || path.starts_with("/org/")
        || path.starts_with("/admin")
        || path.starts_with("/invitations")
        || path.starts_with("/auth/callback")
}

#[cfg(test)]
mod tests {
    use super::{is_workspace_path, is_workspace_settings_path, workspace_topbar_title};

    #[test]
    fn onboarding_uses_the_focused_layout() {
        assert!(!is_workspace_path("/onboarding/workspace"));
        assert!(!is_workspace_path("/onboarding/workspace/"));
        assert!(is_workspace_path("/dashboard"));
        assert!(is_workspace_path("/organizations"));
    }

    #[test]
    fn settings_paths_use_settings_shell_not_workspace_rail() {
        assert!(is_workspace_settings_path("/org/acme/settings"));
        assert!(is_workspace_settings_path("/org/acme/settings/"));
        assert!(is_workspace_settings_path("/org/acme/settings/general"));
        assert!(is_workspace_settings_path("/org/acme/settings/members"));
        assert!(is_workspace_settings_path("/org/acme/settings/invitations"));
        assert!(is_workspace_settings_path("/org/acme/settings/roles"));
        assert!(is_workspace_settings_path("/org/acme/settings/audit"));
        assert!(is_workspace_settings_path("/org/acme/settings/danger"));
        assert!(!is_workspace_settings_path("/org/acme/vault"));
        assert!(!is_workspace_settings_path("/organizations/settings"));
        assert!(!is_workspace_settings_path("/settings"));
        assert!(!is_workspace_settings_path("/org/settings"));
        assert!(!is_workspace_settings_path("/org/acme/settings-extra"));

        assert!(!is_workspace_path("/org/acme/settings"));
        assert!(!is_workspace_path("/org/acme/settings/general"));
        assert!(!is_workspace_path("/org/acme/settings/invitations"));
        assert!(is_workspace_path("/org/acme/vault"));
    }

    #[test]
    fn settings_topbar_titles_match_areas() {
        assert_eq!(
            workspace_topbar_title("/org/acme/settings/general"),
            "Workspace settings"
        );
        assert_eq!(workspace_topbar_title("/org/acme/settings/members"), "Members");
        assert_eq!(
            workspace_topbar_title("/org/acme/settings/invitations"),
            "Invitations"
        );
        assert_eq!(workspace_topbar_title("/org/acme/settings/roles"), "Roles");
        assert_eq!(workspace_topbar_title("/org/acme/settings/audit"), "Audit");
        assert_eq!(
            workspace_topbar_title("/org/acme/settings/danger"),
            "Danger zone"
        );
    }
}

pub(crate) fn workspace_topbar_title(path: &str) -> &'static str {
    let path = path.trim_end_matches('/');
    if path == "/dashboard" || path.is_empty() {
        "Dashboard"
    } else if path.starts_with("/account/profile") {
        "Profile"
    } else if path.starts_with("/account/password") {
        "Password"
    } else if path.starts_with("/account/providers") {
        "Providers"
    } else if path.starts_with("/account/passkeys") {
        "Passkeys"
    } else if path.starts_with("/account/mfa") {
        "MFA"
    } else if path.starts_with("/account/sessions") {
        "Sessions"
    } else if path.starts_with("/account/vault") || path.contains("/vault") {
        "Secret vault"
    } else if path.starts_with("/onboarding") {
        "Create workspace"
    } else if path.starts_with("/account") {
        "Account"
    } else if is_workspace_settings_path(path) {
        if path.ends_with("/members") {
            "Members"
        } else if path.ends_with("/invitations") {
            "Invitations"
        } else if path.ends_with("/roles") {
            "Roles"
        } else if path.ends_with("/audit") {
            "Audit"
        } else if path.ends_with("/danger") {
            "Danger zone"
        } else {
            "Workspace settings"
        }
    } else if path.starts_with("/organizations/settings") {
        "Workspace settings"
    } else if path.starts_with("/organizations/members") {
        "Members"
    } else if path.starts_with("/organizations/invitations") {
        "Invitations"
    } else if path.starts_with("/organizations/roles") {
        "Roles"
    } else if path.starts_with("/organizations/permissions") {
        "Roles"
    } else if path.starts_with("/organizations/audit") {
        "Audit"
    } else if path.starts_with("/organizations") {
        "Workspaces"
    } else if path.starts_with("/admin/users") {
        "Users"
    } else if path.starts_with("/admin/health") {
        "Health"
    } else if path.starts_with("/admin/policies") {
        "Policies"
    } else if path.starts_with("/admin/auth/signing-keys") {
        "Signing keys"
    } else if path.starts_with("/admin/auth/providers") {
        "Auth providers"
    } else if path.starts_with("/admin/auth/redirects") {
        "Redirects"
    } else if path.starts_with("/admin/authorization") {
        "Authorization"
    } else if path.starts_with("/admin") {
        "Admin"
    } else if path.starts_with("/invitations") {
        "Invitation"
    } else {
        "Workspace"
    }
}
