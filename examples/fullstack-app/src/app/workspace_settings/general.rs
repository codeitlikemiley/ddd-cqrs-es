//! Workspace settings — General (name, immutable slug).
//! Editors land in PR4a; this page is a navigable stub.

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use super::shared::settings_page_stub;
use crate::app::helpers::current_browser_pathname;
use leptos::prelude::*;

#[component]
pub fn WorkspaceSettingsGeneralPage() -> impl IntoView {
    settings_page_stub(
        "General",
        "Workspace name and URL. The slug is fixed after create.",
        "Requires organization.view. Name editing and ownership details arrive in a later release.",
        view! { <WorkspaceSettingsGeneralBody /> },
    )
}

/// Island so slug is read on the client after hydrate.
#[island]
pub fn WorkspaceSettingsGeneralBody() -> impl IntoView {
    let slug = Memo::new(move |_| {
        let path = current_browser_pathname();
        let path = path.trim_end_matches('/');
        path.strip_prefix("/org/")
            .and_then(|rest| rest.split_once('/'))
            .map(|(slug, _)| slug.to_owned())
            .unwrap_or_default()
    });
    view! {
        <dl class="kv workspace-settings-kv">
            <dt>"Workspace URL"</dt>
            <dd class="mono-value">
                {move || {
                    let s = slug.get();
                    if s.is_empty() {
                        "—".to_owned()
                    } else {
                        format!("/org/{s}")
                    }
                }}
            </dd>
            <dt>"Slug"</dt>
            <dd class="mono-value">
                {move || {
                    let s = slug.get();
                    if s.is_empty() { "—".to_owned() } else { s }
                }}
                <span class="workspace-settings-readonly-tag">" read-only"</span>
            </dd>
        </dl>
        <p class="board-muted">
            "Display name updates and other general settings will be editable here."
        </p>
    }
}
