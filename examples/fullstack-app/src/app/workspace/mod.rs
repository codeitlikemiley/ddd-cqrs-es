//! Workspace chrome: shell, switcher, user menu, onboarding.

#![allow(unused_imports)]

#[cfg(feature = "hydrate")]
use crate::app::helpers::mark_active_nav;
use crate::app::helpers::{
    can_view_system_navigation, current_browser_pathname, org_monogram, redirect_browser,
    server_error_text,
};
use crate::app::path::{is_workspace_path, is_workspace_settings_path, workspace_topbar_title};
use crate::app::workspace_settings::WorkspaceSettingsShell;
use crate::app::{
    CreateOrganization, LogoutButton, SelectOrganization, browser_load, get_current_session,
    list_organizations,
};
use leptos::prelude::*;
use leptos_router::components::Outlet;
use leptos_router::hooks::use_location;
#[cfg(feature = "hydrate")]
use wasm_bindgen::prelude::*;
#[cfg(feature = "hydrate")]
use web_sys::window;

#[cfg(feature = "hydrate")]
use crate::app::{bind_user_menu_dismiss, bind_workspace_nav_active, init_workspace_sidebar};

/// Persistent workspace chrome. Children render in the main content outlet.
#[component]
pub fn WorkspaceShell(children: Children) -> impl IntoView {
    let location = use_location();
    let topbar_title =
        Memo::new(move |_| workspace_topbar_title(&location.pathname.get()).to_string());

    view! {
        <WorkspaceOnboardingGate />
        <div class="workspace-shell" id="workspace-shell" data-sidebar="full">
            <script>
                {r#"(function(){try{var s=document.getElementById("workspace-shell");if(!s)return;var m=localStorage.getItem("workspace-sidebar-mode");if(m==="mini"||m==="hidden"||m==="full"){s.setAttribute("data-sidebar",m);}}catch(e){}})();"#}
            </script>
            <input
                type="checkbox"
                id="workspace-nav-toggle"
                class="workspace-nav-toggle"
                aria-controls="workspace-sidebar"
            />
            <label
                class="workspace-nav-backdrop"
                for="workspace-nav-toggle"
                aria-label="Close navigation"
            ></label>
            <aside class="workspace-sidebar" id="workspace-sidebar" aria-label="Workspace">
                <div class="workspace-sidebar-top">
                    <a class="workspace-brand" href="/dashboard" aria-label="Workspace home">
                        <span class="workspace-brand-mark" aria-hidden="true">"d"</span>
                        <span class="workspace-brand-copy">
                            <strong>"wasi-auth"</strong>
                            <small>"workspace"</small>
                        </span>
                    </a>
                    <button
                        type="button"
                        class="workspace-sidebar-rail-toggle"
                        data-sidebar-action="toggle-mini"
                        aria-label="Toggle mini sidebar"
                        title="Toggle mini sidebar"
                    >
                        <span class="workspace-sidebar-rail-icon" aria-hidden="true"></span>
                    </button>
                    <label
                        class="workspace-sidebar-close"
                        for="workspace-nav-toggle"
                        aria-label="Close navigation"
                    >
                        "Close"
                    </label>
                </div>
                <nav class="workspace-nav" aria-label="Authenticated workspace">
                    <a class="workspace-nav-link" href="/dashboard" data-nav="overview" title="Overview">
                        <span class="workspace-nav-icon" aria-hidden="true" data-icon="overview"></span>
                        <span class="workspace-nav-text">"Overview"</span>
                    </a>
                    <a class="workspace-nav-link" href="/organizations" data-nav="organizations" title="Organizations">
                        <span class="workspace-nav-icon" aria-hidden="true" data-icon="organizations"></span>
                        <span class="workspace-nav-text">"Organizations"</span>
                    </a>
                    <WorkspaceSystemNav />
                </nav>
                <div class="workspace-sidebar-foot">
                    <WorkspaceOrgSwitcher />
                    <WorkspaceUserMenu />
                </div>
            </aside>
            <div class="workspace-main">
                <header class="workspace-topbar">
                    <label
                        class="workspace-menu-button workspace-menu-button-mobile"
                        for="workspace-nav-toggle"
                        aria-label="Open navigation"
                        aria-controls="workspace-sidebar"
                    >
                        <span class="workspace-menu-button-bars" aria-hidden="true">
                            <span></span>
                            <span></span>
                            <span></span>
                        </span>
                    </label>
                    <a class="workspace-topbar-brand" href="/dashboard" aria-label="Workspace home">
                        <span class="workspace-brand-mark" aria-hidden="true">"d"</span>
                        <span class="workspace-brand-copy">
                            <strong>"wasi-auth"</strong>
                            <small>"workspace"</small>
                        </span>
                    </a>
                    <div class="workspace-topbar-title">
                        <span class="workspace-topbar-page">{move || topbar_title.get()}</span>
                    </div>
                    <div class="workspace-topbar-org">
                        <WorkspaceOrgSwitcher />
                    </div>
                    <button
                        type="button"
                        class="workspace-menu-button workspace-menu-button-desktop"
                        data-sidebar-action="toggle-visibility"
                        aria-label="Show sidebar"
                        title="Show sidebar (⌘B)"
                    >
                        <span class="workspace-menu-button-bars" aria-hidden="true">
                            <span></span>
                            <span></span>
                            <span></span>
                        </span>
                    </button>
                    <WorkspaceSidebarControls />
                    <WorkspaceNavActive />
                </header>
                <div class="workspace-content">
                    {children()}
                </div>
            </div>
        </div>
    }
}

/// If the user has zero organizations, force focused first-workspace onboarding.
///
/// Island must not call `use_location()` — islands hydrate outside the Router context.
#[island]
pub fn WorkspaceOnboardingGate() -> impl IntoView {
    let orgs = browser_load(list_organizations);
    Effect::new(move |_| {
        let path = current_browser_pathname();
        let path = path.trim_end_matches('/');
        let allow = path.starts_with("/onboarding")
            || path.starts_with("/auth")
            || path.starts_with("/invitations")
            || path.starts_with("/login")
            || path.starts_with("/register");
        if allow {
            return;
        }
        if let Some(Ok(list)) = orgs.get() {
            if list.organizations.is_empty() {
                redirect_browser("/onboarding/workspace");
            }
        }
    });
    view! { <></> }
}

#[component]
pub fn WorkspaceOnboardingPage() -> impl IntoView {
    // Minimal chrome — Linear-style focused create.
    view! {
        <div class="page onboarding-page">
            <header class="page-brand">
                <a class="page-brand-link" href="/" aria-label="wasi-auth home">
                    <span class="page-brand-mark" aria-hidden="true">"d"</span>
                    <span>
                        <strong>"wasi-auth"</strong>
                        <small>"Create your workspace"</small>
                    </span>
                </a>
            </header>
            <WorkspaceOnboardingPanel />
        </div>
    }
}

#[island]
pub fn WorkspaceOnboardingPanel() -> impl IntoView {
    let create_action = ServerAction::<CreateOrganization>::new();
    let pending = create_action.pending();
    let value = create_action.value();
    let name = RwSignal::new(String::new());
    let slug = RwSignal::new(String::new());
    let slug_touched = RwSignal::new(false);
    let client_error = RwSignal::new(None::<String>);

    // First-workspace only. Additional workspaces are created from /organizations.
    let existing = browser_load(list_organizations);
    Effect::new(move |_| {
        if let Some(Ok(list)) = existing.get() {
            if list.organizations.is_empty() {
                return;
            }
            redirect_browser("/dashboard");
        }
    });

    Effect::new(move |_| match value.get() {
        // Product home after first workspace is the board, not the vault.
        Some(Ok(_org)) => redirect_browser("/dashboard"),
        Some(Err(e)) => client_error.set(Some(server_error_text(e))),
        None => {}
    });

    let derive_slug = |raw: &str| -> String {
        let mut out = String::new();
        let mut prev_dash = false;
        for ch in raw.trim().chars() {
            let lower = ch.to_ascii_lowercase();
            if lower.is_ascii_alphanumeric() {
                out.push(lower);
                prev_dash = false;
            } else if !prev_dash && !out.is_empty() {
                out.push('-');
                prev_dash = true;
            }
        }
        out.trim_matches('-').chars().take(48).collect()
    };

    view! {
        <section class="panel onboarding-card">
            <p class="section-label">"Welcome"</p>
            <h1 class="onboarding-title">"Create your workspace"</h1>
            <p class="onboarding-lede">
                "Workspaces hold your team, secret vault, and connectors. "
                "Pick a name and a short URL — you can invite others later."
            </p>
            <div class="onboarding-form">
                <label class="auth-field">
                    <span>"Workspace name"</span>
                    <input
                        class="auth-input"
                        type="text"
                        maxlength="120"
                        placeholder="Acme Inc"
                        prop:value=move || name.get()
                        on:input=move |e| {
                            let v = event_target_value(&e);
                            name.set(v.clone());
                            if !slug_touched.get_untracked() {
                                slug.set(derive_slug(&v));
                            }
                            client_error.set(None);
                        }
                    />
                </label>
                <label class="auth-field">
                    <span>"Workspace URL"</span>
                    <div class="slug-input-group" role="group" aria-label="Workspace URL">
                        <span class="slug-input-prefix" aria-hidden="true">"/org/"</span>
                        <input
                            class="auth-input slug-input-field mono-value"
                            type="text"
                            maxlength="48"
                            placeholder="acme"
                            prop:value=move || slug.get()
                            on:input=move |e| {
                                slug_touched.set(true);
                                slug.set(derive_slug(&event_target_value(&e)));
                                client_error.set(None);
                            }
                        />
                    </div>
                    <span class="board-muted">"Used in links like /org/acme/vault. Letters, numbers, hyphens."</span>
                </label>
                <button
                    type="button"
                    class="primary-button"
                    disabled=move || pending.get() || name.get().trim().is_empty() || slug.get().trim().len() < 2
                    on:click=move |_| {
                        create_action.dispatch(CreateOrganization {
                            name: name.get_untracked().trim().to_owned(),
                            slug: slug.get_untracked().trim().to_owned(),
                        });
                    }
                >
                    {move || if pending.get() { "Creating…" } else { "Create workspace" }}
                </button>
                <p class="error-banner" hidden=move || client_error.get().is_none()>
                    {move || client_error.get().unwrap_or_default()}
                </p>
            </div>
        </section>
    }
}

/// Marks active nav links. Island so it runs on the client and follows SPA navigations.
#[island]
pub fn WorkspaceNavActive() -> impl IntoView {
    Effect::new(move |_| {
        #[cfg(feature = "hydrate")]
        {
            use wasm_bindgen::JsCast;
            use wasm_bindgen::closure::Closure;

            let on_mark = Closure::wrap(Box::new(move |_event: web_sys::Event| {
                if let Some(window) = window() {
                    if let Ok(pathname) = window.location().pathname() {
                        mark_active_nav(&pathname);
                    }
                }
            }) as Box<dyn FnMut(_)>);
            if let Some(window) = window() {
                let _ = window.add_event_listener_with_callback(
                    "workspace-nav-mark",
                    on_mark.as_ref().unchecked_ref(),
                );
                on_mark.forget();
                bind_workspace_nav_active();
                if let Ok(pathname) = window.location().pathname() {
                    mark_active_nav(&pathname);
                }
            }
        }
    });
    view! { <span class="workspace-nav-active-marker" aria-hidden="true"></span> }
}

/// Desktop sidebar modes: full ↔ mini (rail toggle) and show ↔ hide (⌘/Ctrl+B).
#[island]
pub fn WorkspaceSidebarControls() -> impl IntoView {
    Effect::new(move |_| {
        #[cfg(feature = "hydrate")]
        {
            init_workspace_sidebar();
        }
    });
    view! { <span class="workspace-sidebar-controls" aria-hidden="true"></span> }
}

/// Top-bar / sidebar workspace switcher: select org, jump to vault, create workspace.
///
/// Click-away / Escape dismiss is shared with the account flyout via
/// `bindUserMenuDismiss` (also installed from workspace sidebar init).
#[island]
pub fn WorkspaceOrgSwitcher() -> impl IntoView {
    let orgs = browser_load(list_organizations);
    let session = browser_load(get_current_session);
    let select_action = ServerAction::<SelectOrganization>::new();
    let select_pending = select_action.pending();

    #[cfg(feature = "hydrate")]
    Effect::new(move |_| {
        bind_user_menu_dismiss();
    });

    Effect::new(move |_| {
        if matches!(select_action.value().get(), Some(Ok(_))) {
            #[cfg(feature = "hydrate")]
            {
                if let Some(window) = window() {
                    let _ = window.location().reload();
                }
            }
        }
    });

    view! {
        <div class="org-switcher">
            {move || {
                let session = session.get();
                let orgs = orgs.get();
                match (session, orgs) {
                    (Some(Ok(sess)), Some(Ok(list))) if sess.authenticated => {
                        let active_id = sess.tenant_id.clone().filter(|s| !s.trim().is_empty());
                        let active = active_id.as_ref().and_then(|id| {
                            list.organizations.iter().find(|o| o.organization_id == *id).cloned()
                        });
                        let label = active
                            .as_ref()
                            .map(|o| o.name.clone())
                            .unwrap_or_else(|| {
                                if list.organizations.is_empty() {
                                    "No workspace".into()
                                } else {
                                    "Select workspace".into()
                                }
                            });
                        let monogram = active
                            .as_ref()
                            .map(|o| org_monogram(&o.name))
                            .unwrap_or_else(|| "W".into());
                        let vault_href = active
                            .as_ref()
                            .map(|o| {
                                if o.slug.is_empty() {
                                    "/account/vault".into()
                                } else {
                                    format!("/org/{}/vault", o.slug)
                                }
                            })
                            .unwrap_or_else(|| "/organizations".into());
                        let orgs_for_list = list.organizations.clone();

                        view! {
                            <details class="org-switcher-details">
                                <summary class="org-switcher-trigger" aria-label="Switch workspace">
                                    <span class="org-switcher-avatar" aria-hidden="true">{monogram}</span>
                                    <span class="org-switcher-meta">
                                        <span class="org-switcher-label">{label}</span>
                                        <span class="org-switcher-hint">"Workspace"</span>
                                    </span>
                                    <span class="org-switcher-caret" aria-hidden="true"></span>
                                </summary>
                                <div class="org-switcher-panel" role="menu">
                                    <p class="org-switcher-panel-label">"Workspaces"</p>
                                    <ul class="org-switcher-list">
                                        {orgs_for_list.into_iter().map(|org| {
                                            let id = org.organization_id.clone();
                                            let id_select = id.clone();
                                            let is_active = active_id.as_ref().is_some_and(|a| a == &id);
                                            let name = org.name.clone();
                                            let slug_line = if org.slug.is_empty() {
                                                String::new()
                                            } else {
                                                format!("/org/{}", org.slug)
                                            };
                                            view! {
                                                <li>
                                                    <button
                                                        type="button"
                                                        class="org-switcher-item"
                                                        class:is-active=is_active
                                                        role="menuitem"
                                                        disabled=move || select_pending.get() || is_active
                                                        on:click=move |_| {
                                                            if is_active { return; }
                                                            select_action.dispatch(SelectOrganization {
                                                                organization_id: id_select.clone(),
                                                            });
                                                        }
                                                    >
                                                        <span class="org-switcher-item-name">{name}</span>
                                                        <span class="org-switcher-item-meta">
                                                            {if is_active { "Active".into() } else { slug_line }}
                                                        </span>
                                                    </button>
                                                </li>
                                            }
                                        }).collect_view()}
                                    </ul>
                                    <div class="org-switcher-divider" aria-hidden="true"></div>
                                    <a class="org-switcher-link" href="/organizations" role="menuitem">"Manage workspaces"</a>
                                    <a class="org-switcher-link" href=vault_href role="menuitem">"Secret vault"</a>
                                </div>
                            </details>
                        }.into_any()
                    }
                    (Some(Ok(_)), _) | (None, _) => view! {
                        <span class="org-switcher-fallback">"…"</span>
                    }.into_any(),
                    _ => view! {
                        <a class="org-switcher-fallback-link" href="/organizations">"Workspaces"</a>
                    }.into_any(),
                }
            }}
        </div>
    }
}

#[island(lazy)]
pub fn WorkspaceSystemNav() -> impl IntoView {
    let session = browser_load(get_current_session);

    view! {
        <div class="workspace-system-nav">
            {move || match session.get() {
                Some(Ok(session)) if session.authenticated && can_view_system_navigation(&session) => {
                    view! {
                        <p class="workspace-nav-label workspace-nav-label-secondary">"System"</p>
                        <a class="workspace-nav-link" href="/admin/health" data-nav="system" title="Health">
                            <span class="workspace-nav-icon" aria-hidden="true" data-icon="system"></span>
                            <span class="workspace-nav-text">"Health"</span>
                        </a>
                    }.into_any()
                }
                _ => view! {}.into_any(),
            }}
        </div>
    }
}

/// ChatGPT-style account flyout: avatar + email open a menu of settings + sign out.
/// Lives in the left rail foot so the main top bar stays clean.
/// Click-away / Escape dismiss is bound in `bindUserMenuDismiss` (also covers
/// the workspace switcher flyout).
#[island]
pub fn WorkspaceUserMenu() -> impl IntoView {
    let session = browser_load(get_current_session);

    #[cfg(feature = "hydrate")]
    Effect::new(move |_| {
        bind_user_menu_dismiss();
    });

    view! {
        <div class="user-menu">
            {move || match session.get() {
                Some(Ok(session)) if session.authenticated => {
                    let email = session
                        .primary_email
                        .clone()
                        .or_else(|| session.user_id.clone())
                        .unwrap_or_else(|| "Signed in".to_string());
                    let initial = email
                        .chars()
                        .next()
                        .map(|ch| ch.to_ascii_uppercase())
                        .unwrap_or('U');
                    view! {
                        <details class="user-menu-details">
                            <summary class="user-menu-trigger" aria-label="Account menu">
                                <span class="user-menu-avatar" aria-hidden="true">{initial.to_string()}</span>
                                <span class="user-menu-meta">
                                    <span class="user-menu-email">{email.clone()}</span>
                                    <span class="user-menu-hint">"Account"</span>
                                </span>
                                <span class="user-menu-caret" aria-hidden="true"></span>
                            </summary>
                            <div class="user-menu-panel" role="menu">
                                <p class="user-menu-panel-label">"Account"</p>
                                <a class="user-menu-item" href="/account/profile" role="menuitem">"Profile"</a>
                                <a class="user-menu-item" href="/account/password" role="menuitem">"Password"</a>
                                <a class="user-menu-item" href="/account/mfa" role="menuitem">"Authenticator (MFA)"</a>
                                <a class="user-menu-item" href="/account/passkeys" role="menuitem">"Passkeys"</a>
                                <a class="user-menu-item" href="/account/sessions" role="menuitem">"Sessions"</a>
                                <a class="user-menu-item" href="/account/providers" role="menuitem">"Providers"</a>
                                <div class="user-menu-divider" aria-hidden="true"></div>
                                <div class="user-menu-logout">
                                    <LogoutButton />
                                </div>
                            </div>
                        </details>
                    }.into_any()
                }
                Some(Ok(_)) => view! {
                    <a class="user-menu-signin" href="/login">"Sign in"</a>
                }.into_any(),
                Some(Err(_)) => view! {
                    <span class="user-menu-fallback">"Session unavailable"</span>
                }.into_any(),
                None => view! {
                    <span class="user-menu-fallback">"Loading…"</span>
                }.into_any(),
            }}
        </div>
    }
}

/// Root layout: keep shell chrome mounted across navigations within each mode.
#[component]
pub fn AppLayout() -> impl IntoView {
    let location = use_location();
    // Mode only flips when entering/leaving settings vs workspace vs public chrome.
    let layout_mode = Memo::new(move |_| {
        let path = location.pathname.get();
        if is_workspace_settings_path(&path) {
            2_u8 // settings shell
        } else if is_workspace_path(&path) {
            1_u8 // workspace rail
        } else {
            0_u8 // auth / public
        }
    });

    view! {
        {move || match layout_mode.get() {
            2 => view! {
                <WorkspaceSettingsShell>
                    <Outlet />
                </WorkspaceSettingsShell>
            }
            .into_any(),
            1 => view! {
                <WorkspaceShell>
                    <Outlet />
                </WorkspaceShell>
            }
            .into_any(),
            _ => view! {
                <main class="auth-shell">
                    <Outlet />
                </main>
            }
            .into_any(),
        }}
    }
}
