//! Workspace vs public path helpers (layout chrome + topbar titles).

pub(crate) fn is_workspace_path(path: &str) -> bool {
    let path = path.trim_end_matches('/');
    if path.starts_with("/onboarding") {
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
    use super::is_workspace_path;

    #[test]
    fn onboarding_uses_the_focused_layout() {
        assert!(!is_workspace_path("/onboarding/workspace"));
        assert!(!is_workspace_path("/onboarding/workspace/"));
        assert!(is_workspace_path("/dashboard"));
        assert!(is_workspace_path("/organizations"));
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
    } else if path.starts_with("/organizations/settings") {
        "Organization settings"
    } else if path.starts_with("/organizations/members") {
        "Members"
    } else if path.starts_with("/organizations/invitations") {
        "Invitations"
    } else if path.starts_with("/organizations/roles") {
        "Roles"
    } else if path.starts_with("/organizations/permissions") {
        "Permissions"
    } else if path.starts_with("/organizations/audit") {
        "Audit"
    } else if path.starts_with("/organizations") {
        "Organizations"
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
