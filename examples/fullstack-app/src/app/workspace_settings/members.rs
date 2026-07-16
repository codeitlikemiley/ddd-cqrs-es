//! Workspace settings — Members.
//! Editors land in PR4a; this page is a navigable stub.

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use super::shared::settings_page_stub;
use leptos::prelude::*;

#[component]
pub fn WorkspaceSettingsMembersPage() -> impl IntoView {
    settings_page_stub(
        "Members",
        "People who belong to this workspace and their roles.",
        "Requires member.view. The member table and role assignment UI ship in a later release.",
        view! {
            <div class="workspace-settings-empty" role="status">
                <p>"No member list loaded yet."</p>
                <p class="board-muted">
                    "When available, you will review active, blocked, and removed memberships here."
                </p>
            </div>
        },
    )
}
