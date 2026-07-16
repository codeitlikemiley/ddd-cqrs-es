//! Workspace settings — Roles & capabilities.
//! Editors land in PR4c; this page is a navigable stub.

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use super::shared::settings_page_stub;
use leptos::prelude::*;

#[component]
pub fn WorkspaceSettingsRolesPage() -> impl IntoView {
    settings_page_stub(
        "Roles",
        "Built-in and custom roles for this workspace.",
        "Requires role.view. Custom role create/edit/delete ships in a later release.",
        view! {
            <div class="workspace-settings-empty" role="status">
                <p>"Role catalog is not loaded yet."</p>
                <p class="board-muted">
                    "Built-in roles stay immutable; custom roles use the bounded tenant permission catalog."
                </p>
            </div>
        },
    )
}
