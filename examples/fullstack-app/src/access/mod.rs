//! Reusable RBAC capability model for UI and backend.
//!
//! UI filters (nav, buttons, widgets) and server gates share the same
//! [`PermissionId`] and [`AccessRequirement`] types. Client-side checks are
//! presentation only; enforcement always happens on the server.

mod context;
mod features;
mod permission;
mod requirement;
mod routes;

pub use context::{AccessContext, AssuranceLevel, PermissionSet};
pub use features::{
    action_seed_demos, action_vault_create_secret, can_view_any_settings, filter_board_nodes,
    filter_widget_catalog, nav_product_items, nav_settings_items, widget_manage_requirement,
    widget_view_requirement, NavHref, NavItem, NavSection,
};
pub use permission::PermissionId;
pub use requirement::{AccessRequirement, PermissionMode};
pub use routes::permission_for_ui_path;

#[cfg(test)]
mod tests;
