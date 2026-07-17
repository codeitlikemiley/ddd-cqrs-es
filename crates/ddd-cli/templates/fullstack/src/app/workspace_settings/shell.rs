//! Workspace settings nested route shell (content only — chrome is WorkspaceShell).

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use super::shared::slug_from_settings_pathname;
use crate::app::helpers::current_browser_pathname;
use crate::ui::classes::{PANEL, RESULT_LINE, WS_REDIRECT, with_extra};
use leptos::prelude::*;
use leptos_router::components::Outlet;
use leptos_router::hooks::{use_location, use_params_map};

/// Resolve workspace slug for settings: route param first, pathname fallback.
///
/// Call from **route components** (not islands) so `use_params_map` works on SSR
/// and the value can be passed into islands as a prop.
pub(crate) fn settings_slug_signal() -> Memo<String> {
    let params = use_params_map();
    let location = use_location();
    Memo::new(move |_| {
        let from_params = params
            .get()
            .get("slug")
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        if let Some(slug) = from_params {
            return slug;
        }
        let from_router = slug_from_settings_pathname(&location.pathname.get());
        if !from_router.is_empty() {
            return from_router;
        }
        slug_from_settings_pathname(&current_browser_pathname())
    })
}

/// Nested parent for `/org/:slug/settings/*` — chrome lives in `WorkspaceShell`.
#[component]
pub fn WorkspaceSettingsShell() -> impl IntoView {
    view! { <Outlet /> }
}

/// Index `/org/:slug/settings` → `…/general`.
#[component]
pub fn WorkspaceSettingsIndexRedirect() -> impl IntoView {
    let slug = settings_slug_signal();
    Effect::new(move |_| {
        let slug = slug.get();
        if slug.is_empty() {
            return;
        }
        crate::app::helpers::redirect_browser(&format!("/org/{slug}/settings/general"));
    });
    view! {
        <section class=with_extra(PANEL, Some(WS_REDIRECT))>
            <p class=RESULT_LINE>"Opening general settings…"</p>
        </section>
    }
}
