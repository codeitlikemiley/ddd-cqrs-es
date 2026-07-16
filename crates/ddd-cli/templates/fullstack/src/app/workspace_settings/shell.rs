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
    WSS_CONTENT, WSS_FOOT_LINK, WSS_IDENTITY, WSS_IDENTITY_COPY, WSS_MAIN, WSS_MENU_BUTTON, WSS_NAV,
    WSS_NAV_BACKDROP, WSS_NAV_LINK, WSS_NAV_TOGGLE, WSS_SHELL, WSS_SIDEBAR, WSS_SIDEBAR_CLOSE,
    WSS_SIDEBAR_FOOT, WSS_SIDEBAR_TOP, WSS_SKELETON_BLOCK, WSS_SKELETON_LINE, WSS_SKELETON_LINE_SM,
    WSS_TOPBAR, WSS_TOPBAR_TITLE, settings_avatar_class, with_extra,
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
        <div class=WSS_SHELL id="workspace-settings-shell" data-testid="workspace-settings-shell">
            <input
                type="checkbox"
                id="workspace-settings-nav-toggle"
                class=WSS_NAV_TOGGLE
                aria-controls="workspace-settings-sidebar"
            />
            <label
                class=WSS_NAV_BACKDROP
                for="workspace-settings-nav-toggle"
                aria-label="Close settings navigation"
            ></label>

            <WorkspaceSettingsSidebar slug=slug active=active />

            <div class=WSS_MAIN>
                <header class=WSS_TOPBAR>
                    <label
                        class=WSS_MENU_BUTTON
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
                    <div class=WSS_TOPBAR_TITLE>
                        <span class=ORG_KICKER>"Workspace"</span>
                        <strong>"Settings"</strong>
                    </div>
                </header>

                <div class=WSS_CONTENT>
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
            class=WSS_SIDEBAR
            id="workspace-settings-sidebar"
            aria-label="Workspace settings"
        >
            <div class=WSS_SIDEBAR_TOP>
                {move || {
                    if loading.get() && identity.get().is_none() {
                        return view! {
                            <div class=WSS_IDENTITY aria-busy="true">
                                <div class=WSS_SKELETON_BLOCK></div>
                                <div class=WSS_IDENTITY_COPY>
                                    <span class=WSS_SKELETON_LINE></span>
                                    <span class=WSS_SKELETON_LINE_SM></span>
                                </div>
                            </div>
                        }
                        .into_any();
                    }
                    match identity.get() {
                        Some((name, slug_val)) => {
                            let monogram = org_monogram(&name);
                            let tone = org_tone_index(&name);
                            let avatar_class = settings_avatar_class(tone);
                            view! {
                                <div class=WSS_IDENTITY>
                                    <div
                                        class=avatar_class
                                        data-tone=tone.to_string()
                                        aria-hidden="true"
                                    >
                                        {monogram}
                                    </div>
                                    <div class=WSS_IDENTITY_COPY>
                                        <strong>{name}</strong>
                                        <small class=MONO_VALUE>{format!("/org/{slug_val}")}</small>
                                    </div>
                                </div>
                            }
                            .into_any()
                        }
                        None => view! {
                            <div class=WSS_IDENTITY>
                                <div class=settings_avatar_class(0) data-tone="0" aria-hidden="true">
                                    "W"
                                </div>
                                <div class=WSS_IDENTITY_COPY>
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
                    class=WSS_SIDEBAR_CLOSE
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

            <nav class=WSS_NAV aria-label="Settings sections">
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
                                    class=WSS_NAV_LINK
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

            <div class=WSS_SIDEBAR_FOOT>
                <a class=WSS_FOOT_LINK href=move || open_workspace_href.get()>
                    "Open workspace"
                </a>
                <a class=WSS_FOOT_LINK href="/organizations">
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
