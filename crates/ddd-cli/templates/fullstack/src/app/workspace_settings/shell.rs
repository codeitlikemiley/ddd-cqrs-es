//! Linear-style workspace settings shell (settings sidebar, not global rail).

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use super::shared::{SettingsSection, slug_from_settings_pathname};
use crate::app::helpers::{
    current_browser_pathname, org_monogram, org_tone_index, server_error_text,
};
use crate::app::{browser_load, get_workspace_settings_context, list_organizations};
use crate::ui::classes::{
    BANNER_ERROR, MONO_VALUE, ORG_KICKER, PANEL, RESULT_LINE, WS_MENU_BAR, WS_MENU_BARS, WS_REDIRECT,
    with_extra,
};
use leptos::prelude::*;
use leptos_router::components::Outlet;
use leptos_router::hooks::{use_location, use_params_map};

/// Resolve workspace slug for settings: route param first, pathname fallback.
///
/// Nested under `/org/:slug/settings`, so `use_params_map` sees `slug`. Islands
/// outside the router still fall back to `window.location.pathname`.
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

/// Settings chrome: identity + section nav + main outlet.
///
/// Mounted as a nested `ParentRoute` view for `/org/:slug/settings/*`.
#[component]
pub fn WorkspaceSettingsShell() -> impl IntoView {
    let slug = settings_slug_signal();
    let location = use_location();
    let active = Memo::new(move |_| {
        SettingsSection::from_path(&location.pathname.get()).unwrap_or(SettingsSection::General)
    });

    view! {
        <div class="workspace-settings-shell" id="workspace-settings-shell" data-testid="workspace-settings-shell">
            <input
                type="checkbox"
                id="workspace-settings-nav-toggle"
                class="workspace-settings-nav-toggle"
                aria-controls="workspace-settings-sidebar"
            />
            <label
                class="workspace-settings-nav-backdrop"
                for="workspace-settings-nav-toggle"
                aria-label="Close settings navigation"
            ></label>

            <WorkspaceSettingsSidebar slug=slug active=active />

            <div class="workspace-settings-main">
                <header class="workspace-settings-topbar">
                    <label
                        class="workspace-settings-menu-button"
                        for="workspace-settings-nav-toggle"
                        aria-label="Open settings navigation"
                        aria-controls="workspace-settings-sidebar"
                    >
                        <span class=WS_MENU_BARS aria-hidden="true">
                            <span class=WS_MENU_BAR></span>
                            <span class=WS_MENU_BAR></span>
                            <span class=WS_MENU_BAR></span>
                        </span>
                    </label>
                    <div class="workspace-settings-topbar-title">
                        <span class=ORG_KICKER>"Workspace"</span>
                        <strong>"Settings"</strong>
                    </div>
                </header>

                <div class="workspace-settings-content">
                    <Outlet />
                </div>
            </div>
        </div>
    }
}

/// Sidebar: loads org identity + builds section nav from the resolved slug.
///
/// Not an island — must stay in Router context for params/location. Server
/// data still loads via hydrate-only `browser_load`.
#[component]
pub fn WorkspaceSettingsSidebar(
    slug: Memo<String>,
    active: Memo<SettingsSection>,
) -> impl IntoView {
    // Load once slug is non-empty (params available under nested ParentRoute).
    let (settings_ctx, set_settings_ctx) = signal(
        None::<Result<crate::contracts::WorkspaceSettingsContext, server_fn::ServerFnError>>,
    );
    Effect::new(move |_| {
        let slug_now = slug.get();
        if slug_now.is_empty() {
            return;
        }
        #[cfg(feature = "hydrate")]
        {
            use leptos::task::spawn_local;
            spawn_local(async move {
                let result = get_workspace_settings_context(slug_now).await;
                set_settings_ctx.set(Some(result));
            });
        }
        #[cfg(not(feature = "hydrate"))]
        {
            let _ = set_settings_ctx;
        }
    });

    let orgs = browser_load(list_organizations);

    let identity = Memo::new(move |_| {
        let slug_now = slug.get();
        if let Some(Ok(ctx)) = settings_ctx.get() {
            return Some((ctx.organization.name, ctx.organization.slug));
        }
        match orgs.get() {
            Some(Ok(list)) => list
                .organizations
                .into_iter()
                .find(|o| o.slug == slug_now)
                .map(|org| (org.name, org.slug))
                .or_else(|| {
                    if slug_now.is_empty() {
                        None
                    } else {
                        Some((slug_now.clone(), slug_now))
                    }
                }),
            Some(Err(_)) | None => {
                if slug_now.is_empty() {
                    None
                } else {
                    Some((slug_now.clone(), slug_now))
                }
            }
        }
    });

    let loading = Memo::new(move |_| settings_ctx.get().is_none() && orgs.get().is_none());
    let list_error = Memo::new(move |_| {
        if let Some(Err(error)) = settings_ctx.get() {
            return Some(server_error_text(error));
        }
        orgs.get().and_then(|result| match result {
            Ok(_) => None,
            Err(error) => Some(server_error_text(error)),
        })
    });

    let open_workspace_href = Memo::new(move |_| {
        let slug_now = slug.get();
        if slug_now.is_empty() {
            "/dashboard".to_owned()
        } else {
            format!("/org/{slug_now}/vault")
        }
    });

    view! {
        <aside
            class="workspace-settings-sidebar"
            id="workspace-settings-sidebar"
            aria-label="Workspace settings"
        >
            <div class="workspace-settings-sidebar-top">
                {move || {
                    if loading.get() && identity.get().is_none() {
                        return view! {
                            <div class="workspace-settings-identity is-skeleton" aria-busy="true">
                                <div class="workspace-settings-avatar skeleton-block"></div>
                                <div class="workspace-settings-identity-copy">
                                    <span class="skeleton-line"></span>
                                    <span class="skeleton-line skeleton-line-sm"></span>
                                </div>
                            </div>
                        }
                        .into_any();
                    }
                    match identity.get() {
                        Some((name, slug_val)) => {
                            let monogram = org_monogram(&name);
                            let tone = org_tone_index(&name);
                            view! {
                                <div class="workspace-settings-identity">
                                    <div
                                        class="workspace-settings-avatar"
                                        data-tone=tone.to_string()
                                        aria-hidden="true"
                                    >
                                        {monogram}
                                    </div>
                                    <div class="workspace-settings-identity-copy">
                                        <strong>{name}</strong>
                                        <small class=MONO_VALUE>{format!("/org/{slug_val}")}</small>
                                    </div>
                                </div>
                            }
                            .into_any()
                        }
                        None => view! {
                            <div class="workspace-settings-identity">
                                <div class="workspace-settings-avatar" data-tone="0" aria-hidden="true">
                                    "W"
                                </div>
                                <div class="workspace-settings-identity-copy">
                                    <strong>"Workspace"</strong>
                                    <small class=MONO_VALUE>
                                        {move || {
                                            let s = slug.get();
                                            if s.is_empty() {
                                                "…".to_owned()
                                            } else {
                                                format!("/org/{s}")
                                            }
                                        }}
                                    </small>
                                </div>
                            </div>
                        }
                        .into_any(),
                    }
                }}
                <label
                    class="workspace-settings-sidebar-close"
                    for="workspace-settings-nav-toggle"
                    aria-label="Close navigation"
                >
                    "Close"
                </label>
            </div>

            <Show when=move || list_error.get().is_some()>
                <p class=with_extra(BANNER_ERROR, Some("mx-1 mb-2 mt-0 px-2.5 py-2 text-xs"))>
                    {move || list_error.get().unwrap_or_default()}
                </p>
            </Show>

            <nav class="workspace-settings-nav" aria-label="Settings sections">
                {move || {
                    let slug_now = slug.get();
                    let current = active.get();
                    SettingsSection::all()
                        .iter()
                        .copied()
                        .map(|section| {
                            let href = section.href(&slug_now);
                            let is_active = section == current;
                            let label = section.label();
                            let disabled = slug_now.is_empty();
                            view! {
                                <a
                                    class="workspace-settings-nav-link"
                                    class:is-active=is_active
                                    class:is-disabled=disabled
                                    href=href
                                    aria-disabled=disabled
                                >
                                    {label}
                                </a>
                            }
                        })
                        .collect_view()
                }}
            </nav>

            <div class="workspace-settings-sidebar-foot">
                <a class="workspace-settings-foot-link" href=move || open_workspace_href.get()>
                    "Open workspace"
                </a>
                <a class="workspace-settings-foot-link" href="/organizations">
                    "Back to workspaces"
                </a>
            </div>
        </aside>
    }
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
