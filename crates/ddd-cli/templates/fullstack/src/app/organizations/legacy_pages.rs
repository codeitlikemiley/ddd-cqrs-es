//! Legacy `/organizations/*` management routes → slug-scoped settings redirects.

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use crate::app::workspace_settings::LegacySettingsRedirect;
use crate::ui::page_shell;
use leptos::prelude::*;

#[component]
pub fn OrganizationSettingsPage() -> impl IntoView {
    page_shell(
        "Workspace settings",
        "Opening the settings for your active workspace…",
        view! { <LegacySettingsRedirect section="general".to_owned() /> },
    )
}

#[component]
pub fn OrganizationMembersPage() -> impl IntoView {
    page_shell(
        "Members",
        "Opening workspace members…",
        view! { <LegacySettingsRedirect section="members".to_owned() /> },
    )
}

#[component]
pub fn OrganizationInvitationsPage() -> impl IntoView {
    page_shell(
        "Invitations",
        "Opening workspace invitations…",
        view! { <LegacySettingsRedirect section="invitations".to_owned() /> },
    )
}

#[component]
pub fn OrganizationRolesPage() -> impl IntoView {
    page_shell(
        "Roles",
        "Opening workspace roles…",
        view! { <LegacySettingsRedirect section="roles".to_owned() /> },
    )
}

#[component]
pub fn OrganizationPermissionsPage() -> impl IntoView {
    page_shell(
        "Roles",
        "Permissions are managed under workspace roles…",
        view! { <LegacySettingsRedirect section="roles".to_owned() /> },
    )
}

#[component]
pub fn OrganizationAuditPage() -> impl IntoView {
    page_shell(
        "Audit",
        "Opening workspace audit log…",
        view! { <LegacySettingsRedirect section="audit".to_owned() /> },
    )
}
