//! Composable access policies for UI and backend.

use super::context::{AccessContext, AssuranceLevel};
use super::permission::PermissionId;

/// How multiple permissions are combined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionMode {
    All,
    Any,
}

/// Declarative requirement attached to nav items, buttons, widgets, and routes.
#[derive(Clone, Debug)]
pub enum AccessRequirement {
    /// Any authenticated session (workspace chrome).
    Authenticated,
    /// Every listed permission.
    AllPermissions(&'static [PermissionId]),
    /// At least one listed permission.
    AnyPermission(&'static [PermissionId]),
    /// Permissions plus minimum assurance (step-up).
    WithAssurance {
        permissions: &'static [PermissionId],
        mode: PermissionMode,
        min_assurance: AssuranceLevel,
    },
    /// System administrator rail (matches historical `can_view_system_navigation`).
    SystemNavigator,
}

impl AccessRequirement {
    #[must_use]
    pub fn is_satisfied_by(&self, ctx: &AccessContext) -> bool {
        match self {
            Self::Authenticated => ctx.authenticated,
            Self::AllPermissions(ids) => ctx.authenticated && ctx.permissions.contains_all(ids),
            Self::AnyPermission(ids) => {
                ctx.authenticated && (ids.is_empty() || ctx.permissions.contains_any(ids))
            }
            Self::WithAssurance {
                permissions,
                mode,
                min_assurance,
            } => {
                if !ctx.authenticated || !ctx.assurance.satisfies(*min_assurance) {
                    return false;
                }
                match mode {
                    PermissionMode::All => ctx.permissions.contains_all(permissions),
                    PermissionMode::Any => {
                        permissions.is_empty() || ctx.permissions.contains_any(permissions)
                    }
                }
            }
            Self::SystemNavigator => {
                ctx.authenticated
                    && (system_admin_aal2(ctx) || ctx.permissions.has_system_prefix())
            }
        }
    }
}

fn system_admin_aal2(ctx: &AccessContext) -> bool {
    ctx.system_administrator && ctx.assurance.satisfies(AssuranceLevel::Aal2)
}
