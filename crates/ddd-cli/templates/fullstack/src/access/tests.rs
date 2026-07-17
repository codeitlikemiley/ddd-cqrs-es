//! Unit tests for pure access evaluation.

use super::context::{AccessContext, AssuranceLevel, PermissionSet};
use super::permission::PermissionId;
use super::requirement::{AccessRequirement, PermissionMode};

fn ctx(authenticated: bool, perms: &[&str], aal: AssuranceLevel) -> AccessContext {
    AccessContext {
        authenticated,
        permissions: PermissionSet::from_iter(perms.iter().copied()),
        assurance: aal,
        system_administrator: false,
    }
}

#[test]
fn permission_set_any_all() {
    let set = PermissionSet::from_iter(["member.view", "role.view"]);
    assert!(set.contains(PermissionId::MEMBER_VIEW));
    assert!(set.contains_all(&[PermissionId::MEMBER_VIEW, PermissionId::ROLE_VIEW]));
    assert!(!set.contains_all(&[PermissionId::MEMBER_VIEW, PermissionId::AUDIT_VIEW]));
    assert!(set.contains_any(&[PermissionId::AUDIT_VIEW, PermissionId::ROLE_VIEW]));
}

#[test]
fn authenticated_requirement() {
    assert!(!AccessRequirement::Authenticated.is_satisfied_by(&ctx(false, &[], AssuranceLevel::Aal1)));
    assert!(AccessRequirement::Authenticated.is_satisfied_by(&ctx(true, &[], AssuranceLevel::Aal1)));
}

#[test]
fn with_assurance_requires_aal2() {
    let req = AccessRequirement::WithAssurance {
        permissions: &[PermissionId::VAULT_MANAGE],
        mode: PermissionMode::All,
        min_assurance: AssuranceLevel::Aal2,
    };
    assert!(!req.is_satisfied_by(&ctx(true, &["vault.manage"], AssuranceLevel::Aal1)));
    assert!(req.is_satisfied_by(&ctx(true, &["vault.manage"], AssuranceLevel::Aal2)));
}

#[test]
fn system_navigator_accepts_system_permission_prefix() {
    let with_system = AccessContext {
        authenticated: true,
        permissions: PermissionSet::from_iter(["system.health.read"]),
        assurance: AssuranceLevel::Aal1,
        system_administrator: false,
    };
    assert!(AccessRequirement::SystemNavigator.is_satisfied_by(&with_system));
}
