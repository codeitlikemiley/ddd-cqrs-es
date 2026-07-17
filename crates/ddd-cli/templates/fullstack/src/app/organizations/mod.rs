//! Organization management UI.
//!
//! - [`OrganizationsPage`] / [`OrganizationsHome`]: workspace switcher + create modal
//! - Legacy `/organizations/*` routes redirect into slug-scoped settings (PR2)

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

mod create_modal;
mod home;
mod legacy_pages;
mod links;

pub use create_modal::CreateOrganizationModal;
pub use home::OrganizationsHome;
pub use legacy_pages::{
    OrganizationAuditPage, OrganizationInvitationsPage, OrganizationMembersPage,
    OrganizationPermissionsPage, OrganizationRolesPage, OrganizationSettingsPage,
};
pub use links::OrganizationLinks;

use crate::ui::page_shell;
use leptos::prelude::*;

#[island(lazy)]
pub fn OrganizationsPage() -> impl IntoView {
    page_shell(
        "Workspaces",
        "Workspaces you belong to. Select one to scope members, roles, and audit.",
        view! { <OrganizationsHome /> },
    )
}
