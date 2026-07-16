//! Workspace settings shell chrome (settings sidebar layout).
//!
//! Real shell lands in PR2.

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use leptos::prelude::*;

/// Placeholder for the slug-scoped settings shell.
/// Not routed yet (PR2).
#[component]
pub fn WorkspaceSettingsShellPlaceholder() -> impl IntoView {
    view! {
        <div class="workspace-settings-shell-placeholder" data-pr="PR0">
            <p class="result-line">"Workspace settings shell (coming soon)"</p>
        </div>
    }
}
