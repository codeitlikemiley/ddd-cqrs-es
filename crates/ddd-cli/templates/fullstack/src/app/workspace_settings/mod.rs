//! Slug-scoped workspace settings UI (Linear-style).
//!
//! Scaffolded in PR0; routes and real pages land in PR2+.

pub mod audit;
pub mod danger;
pub mod general;
pub mod invitations;
pub mod members;
pub mod roles;
pub mod shared;
pub mod shell;

pub use shell::WorkspaceSettingsShellPlaceholder;
