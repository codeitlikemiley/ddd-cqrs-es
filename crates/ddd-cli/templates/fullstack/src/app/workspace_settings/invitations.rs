//! Workspace settings — Invitations.
//! Editors land in PR4b; this page is a navigable stub.

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use super::shared::settings_page_stub;
use leptos::prelude::*;

#[component]
pub fn WorkspaceSettingsInvitationsPage() -> impl IntoView {
    settings_page_stub(
        "Invitations",
        "Pending invites to join this workspace.",
        "Requires member.view. Invite, resend, and revoke actions ship in a later release.",
        view! {
            <div class="workspace-settings-empty" role="status">
                <p>"No invitations to show yet."</p>
                <p class="board-muted">
                    "One-time invitation values are mailed; only hashes are persisted."
                </p>
            </div>
        },
    )
}
