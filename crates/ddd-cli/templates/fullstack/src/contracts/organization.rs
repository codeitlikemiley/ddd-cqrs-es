#![allow(unused_imports)]
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrganizationSummary {
    pub organization_id: String,
    pub name: String,
    /// Unique URL key for `/org/{slug}/…` (empty if not registered yet).
    #[serde(default)]
    pub slug: String,
    pub status: String,
    pub current_user_role: String,
    pub permissions: Vec<String>,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrganizationListResponse {
    pub organizations: Vec<OrganizationSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrganizationCreateRequest {
    pub name: String,
    /// Unique URL slug (`acme`). Auto-derived from name when empty.
    #[serde(default)]
    pub slug: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrganizationUpdateRequest {
    pub organization_id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrganizationSelectRequest {
    pub organization_id: String,
}

/// Result of copying legacy per-user dashboard KV into a workspace.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceLegacyMigrateReport {
    pub organization_id: String,
    pub dry_run: bool,
    pub board_copied: bool,
    pub secrets_copied: bool,
    pub secret_rows_copied: u32,
    pub secret_rows_skipped_reenter: u32,
    /// Secret keys that still need manual re-entry (ciphertext under old AAD).
    #[serde(default)]
    pub reenter_required_keys: Vec<String>,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceLegacyMigrateRequest {
    pub organization_id: String,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MembershipSummary {
    pub organization_id: String,
    pub user_id: String,
    pub primary_email: String,
    pub role_id: String,
    pub status: String,
    pub joined_at_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MembershipListResponse {
    pub memberships: Vec<MembershipSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MembershipRoleRequest {
    pub organization_id: String,
    pub user_id: String,
    pub role_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MembershipRemoveRequest {
    pub organization_id: String,
    pub user_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InvitationCreateRequest {
    pub organization_id: String,
    pub email: String,
    pub role_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InvitationAcceptRequest {
    pub token: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InvitationSummary {
    pub invitation_id: String,
    pub organization_id: String,
    pub email: String,
    pub role_id: String,
    pub status: String,
    pub expires_at_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InvitationListResponse {
    pub invitations: Vec<InvitationSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoleSummary {
    pub organization_id: String,
    pub role_id: String,
    pub name: String,
    pub built_in: bool,
    pub permissions: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoleListResponse {
    pub roles: Vec<RoleSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoleUpsertRequest {
    pub organization_id: String,
    pub role_id: String,
    pub name: String,
    pub permissions: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PermissionCatalogResponse {
    pub permissions: Vec<String>,
}

/// Organization slice for workspace settings chrome and general page.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceSettingsOrganization {
    pub organization_id: String,
    pub name: String,
    pub slug: String,
    pub status: String,
    pub created_at_ms: u64,
}

/// Caller's membership in the resolved workspace (active only).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceSettingsMembership {
    pub role_id: String,
    pub role_name: String,
    pub status: String,
}

/// Role option for assign/invite comboboxes (owner excluded for ordinary assign).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceRoleOption {
    pub role_id: String,
    pub name: String,
    pub built_in: bool,
}

/// Slug-scoped settings bootstrap payload for the caller.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceSettingsContext {
    pub organization: WorkspaceSettingsOrganization,
    pub membership: WorkspaceSettingsMembership,
    /// Effective permissions for the caller in this workspace.
    pub capabilities: Vec<String>,
    /// Roles suitable for assign/invite comboboxes (excludes `owner`).
    pub role_options: Vec<WorkspaceRoleOption>,
    pub member_count: u32,
    pub pending_invitation_count: u32,
    /// True when session assurance is below AAL2 (mutations need step-up).
    pub requires_step_up: bool,
}
