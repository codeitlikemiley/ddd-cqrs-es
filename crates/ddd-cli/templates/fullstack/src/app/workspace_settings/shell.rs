//! Linear-style workspace settings shell (settings sidebar, not global rail).

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use super::shared::SettingsSection;
use crate::app::helpers::{
    current_browser_pathname, org_monogram, org_tone_index, server_error_text,
};
use crate::app::{browser_load, list_organizations};
use leptos::prelude::*;

/// Settings chrome: identity + section nav + main outlet.
///
/// Mounted by [`crate::app::workspace::AppLayout`] when the path is under
/// `/org/{slug}/settings`. Children come from the layout `<Outlet/>`.
#[component]
pub fn WorkspaceSettingsShell(children: Children) -> impl IntoView {
    view! {
        <div class="workspace-settings-shell" id="workspace-settings-shell">
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

            <WorkspaceSettingsSidebar />

            <div class="workspace-settings-main">
                <header class="workspace-settings-topbar">
                    <label
                        class="workspace-settings-menu-button"
                        for="workspace-settings-nav-toggle"
                        aria-label="Open settings navigation"
                        aria-controls="workspace-settings-sidebar"
                    >
                        <span class="workspace-menu-button-bars" aria-hidden="true">
                            <span></span>
                            <span></span>
                            <span></span>
                        </span>
                    </label>
                    <div class="workspace-settings-topbar-title">
                        <span class="dash-eyebrow">"Workspace"</span>
                        <strong>"Settings"</strong>
                    </div>
                </header>

                <div class="workspace-settings-content">
                    {children()}
                </div>
            </div>
        </div>
    }
}

/// Sidebar island: loads org identity + builds section nav from the URL slug.
#[island]
pub fn WorkspaceSettingsSidebar() -> impl IntoView {
    let path = RwSignal::new(current_browser_pathname());
    #[cfg(feature = "hydrate")]
    {
        Effect::new(move |_| {
            // Re-read on mount; soft navigations that remount the island refresh too.
            path.set(current_browser_pathname());
        });
    }

    let slug = Memo::new(move |_| slug_from_settings_path(&path.get()));
    let active = Memo::new(move |_| {
        SettingsSection::from_path(&path.get()).unwrap_or(SettingsSection::General)
    });
    let orgs = browser_load(list_organizations);

    let identity = Memo::new(move |_| {
        let slug_now = slug.get();
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

    let loading = Memo::new(move |_| orgs.get().is_none());
    let list_error = Memo::new(move |_| {
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
                                        <small class="mono-value">{format!("/org/{slug_val}")}</small>
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
                <p class="error-banner workspace-settings-sidebar-error">
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
                            let href_for_click = href.clone();
                            let is_active = section == current;
                            let label = section.label();
                            view! {
                                <a
                                    class="workspace-settings-nav-link"
                                    class:is-active=is_active
                                    href=href
                                    on:click=move |_| {
                                        // Optimistic active state for in-app navigations.
                                        path.set(href_for_click.clone());
                                    }
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

fn slug_from_settings_path(path: &str) -> String {
    let path = path.trim_end_matches('/');
    let Some(rest) = path.strip_prefix("/org/") else {
        return String::new();
    };
    let Some((slug, after)) = rest.split_once('/') else {
        return String::new();
    };
    if after == "settings" || after.starts_with("settings/") {
        slug.to_owned()
    } else {
        String::new()
    }
}

/// Index `/org/:slug/settings` → `…/general`.
///
/// Island: parse pathname (islands hydrate outside Router context).
#[island]
pub fn WorkspaceSettingsIndexRedirect() -> impl IntoView {
    Effect::new(move |_| {
        let path = current_browser_pathname();
        let path = path.trim_end_matches('/');
        let Some(rest) = path.strip_prefix("/org/") else {
            return;
        };
        let Some((slug, after)) = rest.split_once('/') else {
            return;
        };
        if after != "settings" && !after.starts_with("settings/") {
            return;
        }
        if slug.is_empty() {
            return;
        }
        crate::app::helpers::redirect_browser(&format!("/org/{slug}/settings/general"));
    });
    view! {
        <section class="panel workspace-settings-redirect">
            <p class="result-line">"Opening general settings…"</p>
        </section>
    }
}
