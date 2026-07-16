//! Workspace settings — Ownership transfer & danger zone.
//! Actions land in PR4e; this page is a navigable stub.

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use super::shared::settings_page_stub;
use leptos::prelude::*;

#[component]
pub fn WorkspaceSettingsDangerPage() -> impl IntoView {
    settings_page_stub(
        "Danger zone",
        "Irreversible or high-impact workspace actions.",
        "Requires workspace membership. Leave, transfer ownership, and archive ship later.",
        view! {
            <div class="workspace-settings-empty workspace-settings-danger-hint" role="status">
                <p>"No destructive actions are available yet."</p>
                <p class="board-muted">
                    "Ownership transfer, leave workspace, and soft deactivate will appear here with confirmation."
                </p>
            </div>
        },
    )
}
