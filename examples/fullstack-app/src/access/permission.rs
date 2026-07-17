//! Stable product permission identifiers (match wasi-auth catalog strings).

/// Product permission string used by roles, session payloads, and UI gates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PermissionId(pub &'static str);

impl PermissionId {
    // Core tenancy
    pub const AUDIT_VIEW: Self = Self("audit.view");
    pub const MEMBER_INVITE: Self = Self("member.invite");
    pub const MEMBER_MANAGE: Self = Self("member.manage");
    pub const MEMBER_VIEW: Self = Self("member.view");
    pub const ORGANIZATION_UPDATE: Self = Self("organization.update");
    pub const ORGANIZATION_VIEW: Self = Self("organization.view");
    pub const OWNERSHIP_TRANSFER: Self = Self("ownership.transfer");
    pub const ROLE_MANAGE: Self = Self("role.manage");
    pub const ROLE_VIEW: Self = Self("role.view");

    // Application
    pub const COUNTER_CHANGE: Self = Self("counter.change");
    pub const COUNTER_RESET: Self = Self("counter.reset");
    pub const COUNTER_VIEW: Self = Self("counter.view");
    pub const DASHBOARD_MANAGE: Self = Self("dashboard.manage");
    pub const DASHBOARD_VIEW: Self = Self("dashboard.view");
    pub const QUERY_EXECUTE: Self = Self("query.execute");
    pub const QUERY_EXECUTE_MUTATION: Self = Self("query.execute_mutation");
    pub const QUERY_MANAGE: Self = Self("query.manage");
    pub const QUERY_VIEW: Self = Self("query.view");
    pub const RESOURCE_MANAGE: Self = Self("resource.manage");
    pub const RESOURCE_VIEW: Self = Self("resource.view");
    pub const VAULT_MANAGE: Self = Self("vault.manage");
    pub const VAULT_REVEAL: Self = Self("vault.reveal");
    pub const VAULT_VIEW: Self = Self("vault.view");

    // System / admin (session or system-admin paths)
    pub const SYSTEM_USER_MANAGE: Self = Self("system.user.manage");
    pub const SYSTEM_HEALTH_READ: Self = Self("system.health.read");
    pub const SYSTEM_POLICY_MANAGE: Self = Self("system.policy.manage");
    pub const AUTH_SIGNING_KEY_ADMIN: Self = Self("auth:signing-key:admin");
    pub const AUTH_PROVIDER_WRITE: Self = Self("auth:provider:write");
    pub const AUTH_REDIRECT_WRITE: Self = Self("auth:redirect:write");
    pub const AUTHZ_CHECK: Self = Self("authz:check");

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl AsRef<str> for PermissionId {
    fn as_ref(&self) -> &str {
        self.0
    }
}

impl PartialEq<str> for PermissionId {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for PermissionId {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}
