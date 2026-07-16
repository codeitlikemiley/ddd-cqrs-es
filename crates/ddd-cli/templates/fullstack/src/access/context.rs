//! Evaluation context built from session / org permission lists.

use super::permission::PermissionId;
use crate::contracts::SessionView;
use std::collections::HashSet;

/// Session assurance level used for step-up policies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AssuranceLevel {
    None,
    Aal1,
    Aal2,
    Aal3,
}

impl AssuranceLevel {
    #[must_use]
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "aal3" => Self::Aal3,
            "aal2" => Self::Aal2,
            "aal1" => Self::Aal1,
            _ => Self::None,
        }
    }

    #[must_use]
    pub fn satisfies(self, minimum: Self) -> bool {
        self >= minimum
    }
}

/// Set of permission strings granted in the active tenant (or system scope).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PermissionSet {
    inner: HashSet<String>,
}

impl PermissionSet {
    #[must_use]
    pub fn from_iter(permissions: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        Self {
            inner: permissions
                .into_iter()
                .map(|p| p.as_ref().trim().to_owned())
                .filter(|p| !p.is_empty())
                .collect(),
        }
    }

    #[must_use]
    pub fn contains(&self, id: PermissionId) -> bool {
        self.inner.contains(id.as_str())
    }

    #[must_use]
    pub fn contains_any(&self, ids: &[PermissionId]) -> bool {
        ids.iter().any(|id| self.contains(*id))
    }

    #[must_use]
    pub fn contains_all(&self, ids: &[PermissionId]) -> bool {
        ids.iter().all(|id| self.contains(*id))
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// True if any granted permission looks like a system/admin capability.
    #[must_use]
    pub fn has_system_prefix(&self) -> bool {
        self.inner.iter().any(|p| {
            p.starts_with("system.") || p.starts_with("auth:") || p.starts_with("authz:")
        })
    }
}

/// Snapshot of authz facts for pure capability evaluation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessContext {
    pub authenticated: bool,
    pub permissions: PermissionSet,
    pub assurance: AssuranceLevel,
    pub system_administrator: bool,
}

impl AccessContext {
    #[must_use]
    pub fn anonymous() -> Self {
        Self {
            authenticated: false,
            permissions: PermissionSet::default(),
            assurance: AssuranceLevel::None,
            system_administrator: false,
        }
    }

    #[must_use]
    pub fn from_session(session: &SessionView) -> Self {
        Self {
            authenticated: session.authenticated,
            permissions: PermissionSet::from_iter(session.permissions.iter().map(String::as_str)),
            assurance: AssuranceLevel::parse(&session.assurance),
            system_administrator: session.system_administrator,
        }
    }

    /// Build from an org permission list (e.g. dashboard snapshot) with session meta.
    #[must_use]
    pub fn from_permissions(
        authenticated: bool,
        permissions: impl IntoIterator<Item = impl AsRef<str>>,
        assurance: &str,
        system_administrator: bool,
    ) -> Self {
        Self {
            authenticated,
            permissions: PermissionSet::from_iter(permissions),
            assurance: AssuranceLevel::parse(assurance),
            system_administrator,
        }
    }

    #[must_use]
    pub fn has(&self, id: PermissionId) -> bool {
        self.authenticated && self.permissions.contains(id)
    }
}
