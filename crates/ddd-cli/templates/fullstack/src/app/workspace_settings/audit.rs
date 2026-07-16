//! Workspace settings — Audit activity.
//! Humanized UI lands in PR4d; this page is a navigable stub.

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use super::shared::settings_page_stub;
use leptos::prelude::*;

#[component]
pub fn WorkspaceSettingsAuditPage() -> impl IntoView {
    settings_page_stub(
        "Audit log",
        "Security and administration activity for this workspace.",
        "Requires audit.view. Filters, cursor pagination, and detail drawers ship later.",
        view! {
            <div class="workspace-settings-empty" role="status">
                <p>"No audit events loaded yet."</p>
                <p class="board-muted">
                    "Cursor-based audit reads share the same authorization path as the gRPC stream."
                </p>
            </div>
        },
    )
}
