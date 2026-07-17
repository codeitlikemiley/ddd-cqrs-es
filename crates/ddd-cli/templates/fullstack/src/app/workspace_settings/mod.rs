//! Slug-scoped workspace settings UI (Linear-style).

pub mod audit;
pub mod danger;
pub mod general;
pub mod invitations;
pub mod members;
pub mod roles;
pub mod shared;
pub mod shell;

pub use audit::WorkspaceSettingsAuditPage;
pub use danger::WorkspaceSettingsDangerPage;
pub use general::WorkspaceSettingsGeneralPage;
pub use invitations::WorkspaceSettingsInvitationsPage;
pub use members::WorkspaceSettingsMembersPage;
pub use roles::WorkspaceSettingsRolesPage;
pub use shared::LegacySettingsRedirect;
pub use shell::{WorkspaceSettingsIndexRedirect, WorkspaceSettingsShell};
