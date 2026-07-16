//! Workspace chrome: shell, switcher, user menu, onboarding.

#![allow(unused_imports)]

#[cfg(feature = "hydrate")]
use crate::app::helpers::mark_active_nav;
use crate::app::helpers::{
    can_view_system_navigation, current_browser_pathname, org_monogram, redirect_browser,
    server_error_text,
};
use crate::access::{
    can_view_any_settings, nav_product_items, nav_settings_items, AccessContext, NavHref,
};
use crate::app::path::{
    is_workspace_path, is_workspace_settings_path, settings_slug_from_path, workspace_topbar_title,
};
use crate::app::theme::ThemeToggle;

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
use crate::ui::classes::{
    BANNER_ERROR, BTN_PRIMARY, FIELD, INPUT, MONO_VALUE, MUTED, ONBOARDING_CARD, ONBOARDING_FORM,
    ONBOARDING_LEDE, ONBOARDING_PAGE, ONBOARDING_TITLE, ORG_SWITCHER, ORG_SWITCHER_AVATAR,
    ORG_SWITCHER_CARET, ORG_SWITCHER_DETAILS, ORG_SWITCHER_DIVIDER, ORG_SWITCHER_FALLBACK,
    ORG_SWITCHER_HINT, ORG_SWITCHER_ITEM, ORG_SWITCHER_ITEM_META, ORG_SWITCHER_ITEM_NAME,
    ORG_SWITCHER_LABEL, ORG_SWITCHER_LINK, ORG_SWITCHER_LIST, ORG_SWITCHER_META,
    ORG_SWITCHER_PANEL, ORG_SWITCHER_PANEL_LABEL, ORG_SWITCHER_TRIGGER, PAGE_BRAND,
    PAGE_BRAND_LINK, PAGE_BRAND_MARK, SECTION_LABEL, SLUG_INPUT_FIELD, SLUG_INPUT_GROUP,
    SLUG_INPUT_PREFIX, USER_MENU, USER_MENU_AVATAR, USER_MENU_CARET, USER_MENU_DETAILS,
    USER_MENU_DIVIDER, USER_MENU_EMAIL, USER_MENU_FALLBACK, USER_MENU_HINT, USER_MENU_ITEM,
    USER_MENU_LOGOUT, USER_MENU_META, USER_MENU_PANEL, USER_MENU_PANEL_LABEL, USER_MENU_TRIGGER,
    WS_BRAND, WS_BRAND_COPY, WS_BRAND_MARK, WS_CONTENT, WS_HIDDEN_MARKER, WS_MAIN, WS_MENU_BAR,
    WS_MENU_BARS, WS_MENU_BUTTON_DESKTOP, WS_MENU_BUTTON_MOBILE, WS_NAV, WS_NAV_BACKDROP,
    WS_NAV_ICON, WS_NAV_LABEL, WS_NAV_LABEL_SECONDARY, WS_NAV_LINK, WS_NAV_TEXT, WS_NAV_TOGGLE,
    WS_RAIL_ICON, WS_RAIL_ICON_BAR, WS_RAIL_TOGGLE, WS_SHELL, WS_SIDEBAR, WS_SIDEBAR_CLOSE,
    WS_SIDEBAR_FOOT, WS_SIDEBAR_TOP, WS_SYSTEM_NAV, WS_TOPBAR, WS_TOPBAR_BRAND, WS_TOPBAR_ORG,
    WS_TOPBAR_PAGE, WS_TOPBAR_TITLE, with_extra,
};

/// Inline stroke icon for workspace nav (replaces CSS mask icons).
fn nav_icon(kind: &'static str) -> impl IntoView {
    let path = match kind {
        "overview" => {
            view! {
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                    <path d="M3 10.5 12 3l9 7.5"></path>
                    <path d="M5 10v10h14V10"></path>
                </svg>
            }
            .into_any()
        }
        "organizations" => {
            view! {
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                    <path d="M3 21h18"></path>
                    <path d="M5 21V7l7-4 7 4v14"></path>
                    <path d="M9 21v-6h6v6"></path>
                </svg>
            }
            .into_any()
        }
        "system" => {
            view! {
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                    <path d="M12 2v4"></path>
                    <path d="M12 18v4"></path>
                    <path d="m4.9 4.9 2.8 2.8"></path>
                    <path d="m16.3 16.3 2.8 2.8"></path>
                    <path d="M2 12h4"></path>
                    <path d="M18 12h4"></path>
                    <path d="m4.9 19.1 2.8-2.8"></path>
                    <path d="m16.3 7.7 2.8-2.8"></path>
                    <circle cx="12" cy="12" r="3"></circle>
                </svg>
            }
            .into_any()
        }
        _ => {
            view! {
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                    <circle cx="12" cy="12" r="3"></circle>
                </svg>
            }
            .into_any()
        }
    };
    view! {
        <span class=WS_NAV_ICON aria-hidden="true" data-icon=kind>
            {path}
        </span>
    }
}

fn menu_bars() -> impl IntoView {
    view! {
        <span class=WS_MENU_BARS aria-hidden="true">
            <span class=WS_MENU_BAR></span>
            <span class=WS_MENU_BAR></span>
            <span class=WS_MENU_BAR></span>
        </span>
    }
}

/// Persistent workspace chrome. Children render in the main content outlet.
#[component]
pub fn WorkspaceShell(children: Children) -> impl IntoView {
    let location = use_location();
    let topbar_title =
        Memo::new(move |_| workspace_topbar_title(&location.pathname.get()).to_string());

    view! {
        <WorkspaceOnboardingGate />
        <div class=WS_SHELL id="workspace-shell" data-testid="workspace-shell" data-sidebar="full">
            <script class="hidden">
                {r#"(function(){try{var s=document.getElementById("workspace-shell");if(!s)return;var m=localStorage.getItem("workspace-sidebar-mode");if(m==="mini"||m==="hidden"||m==="full"){s.setAttribute("data-sidebar",m);}}catch(e){}})();"#}
            </script>
            <input
                type="checkbox"
                id="workspace-nav-toggle"
                class=WS_NAV_TOGGLE
                aria-controls="workspace-sidebar"
            />
            <label
                class=WS_NAV_BACKDROP
                for="workspace-nav-toggle"
                aria-label="Close navigation"
            ></label>
            <aside class=WS_SIDEBAR id="workspace-sidebar" aria-label="Workspace">
                <div class=WS_SIDEBAR_TOP>
                    <a class=WS_BRAND href="/dashboard" aria-label="Workspace home">
                        <span class=WS_BRAND_MARK aria-hidden="true">"d"</span>
                        <span class=WS_BRAND_COPY>
                            <strong>"wasi-auth"</strong>
                            <small>"workspace"</small>
                        </span>
                    </a>
                    <button
                        type="button"
                        class=WS_RAIL_TOGGLE
                        data-sidebar-action="toggle-mini"
                        aria-label="Toggle mini sidebar"
                        title="Toggle mini sidebar"
                    >
                        <span class=WS_RAIL_ICON aria-hidden="true">
                            <span class=WS_RAIL_ICON_BAR></span>
                        </span>
                    </button>
                    <label
                        class=WS_SIDEBAR_CLOSE
                        for="workspace-nav-toggle"
                        aria-label="Close navigation"
                    >
                        "Close"
                    </label>
                </div>
                <WorkspacePrimaryNav />
                <div class=WS_SIDEBAR_FOOT>
                    <WorkspaceOrgSwitcher />
                    <WorkspaceUserMenu />
                    <ThemeToggle />
                </div>
            </aside>
            <div class=WS_MAIN>
                <header class=WS_TOPBAR>
                    <label
                        class=WS_MENU_BUTTON_MOBILE
                        for="workspace-nav-toggle"
                        aria-label="Open navigation"
                        aria-controls="workspace-sidebar"
                    >
                        {menu_bars()}
                    </label>
                    <a class=WS_TOPBAR_BRAND href="/dashboard" aria-label="Workspace home">
                        <span class=WS_BRAND_MARK aria-hidden="true">"d"</span>
                        <span class=WS_BRAND_COPY>
                            <strong>"wasi-auth"</strong>
                            <small>"workspace"</small>
                        </span>
                    </a>
                    <div class=WS_TOPBAR_TITLE>
                        <span class=WS_TOPBAR_PAGE>{move || topbar_title.get()}</span>
                    </div>
                    <div class=WS_TOPBAR_ORG data-org-placement="top">
                        <WorkspaceOrgSwitcher />
                    </div>
                    <button
                        type="button"
                        class=WS_MENU_BUTTON_DESKTOP
                        data-sidebar-action="toggle-visibility"
                        aria-label="Show sidebar"
                        title="Show sidebar (⌘B)"
                    >
                        {menu_bars()}
                    </button>
                    <WorkspaceSidebarControls />
                    <WorkspaceNavActive />
                </header>
                <div class=WS_CONTENT>
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
        <div class=ONBOARDING_PAGE>
            <header class=PAGE_BRAND>
                <a class=PAGE_BRAND_LINK href="/" aria-label="wasi-auth home">
                    <span class=PAGE_BRAND_MARK aria-hidden="true">"d"</span>
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

    let slug_input_class = with_extra(
        &with_extra(INPUT, Some(SLUG_INPUT_FIELD)),
        Some(MONO_VALUE),
    );

    view! {
        <section class=ONBOARDING_CARD>
            <p class=SECTION_LABEL>"Welcome"</p>
            <h1 class=ONBOARDING_TITLE>"Create your workspace"</h1>
            <p class=ONBOARDING_LEDE>
                "Workspaces hold your team, secret vault, and connectors. "
                "Pick a name and a short URL — you can invite others later."
            </p>
            <div class=ONBOARDING_FORM>
                <label class=FIELD>
                    <span>"Workspace name"</span>
                    <input
                        class=INPUT
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
                <label class=FIELD>
                    <span>"Workspace URL"</span>
                    <div class=SLUG_INPUT_GROUP role="group" aria-label="Workspace URL">
                        <span class=SLUG_INPUT_PREFIX aria-hidden="true">"/org/"</span>
                        <input
                            class=slug_input_class.clone()
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
                    <span class=MUTED>"Used in links like /org/acme/vault. Letters, numbers, hyphens."</span>
                </label>
                <button
                    type="button"
                    class=BTN_PRIMARY
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
                <p class=BANNER_ERROR hidden=move || client_error.get().is_none()>
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
    view! { <span class=WS_HIDDEN_MARKER aria-hidden="true"></span> }
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
    view! { <span class=WS_HIDDEN_MARKER aria-hidden="true"></span> }
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
        <div class=ORG_SWITCHER>
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
                        let settings_href = active
                            .as_ref()
                            .map(|o| {
                                if o.slug.is_empty() {
                                    "/organizations".into()
                                } else {
                                    format!("/org/{}/settings/general", o.slug)
                                }
                            })
                            .unwrap_or_else(|| "/organizations".into());
                        // Org RBAC lives on the membership, not session OAuth scopes.
                        let show_settings = active
                            .as_ref()
                            .map(|org| {
                                can_view_any_settings(&AccessContext::from_permissions(
                                    true,
                                    org.permissions.iter().map(String::as_str),
                                    &sess.assurance,
                                    sess.system_administrator,
                                ))
                            })
                            .unwrap_or(false);
                        let orgs_for_list = list.organizations.clone();

                        view! {
                            <details class=ORG_SWITCHER_DETAILS data-flyout="org-switcher">
                                <summary class=ORG_SWITCHER_TRIGGER aria-label="Switch workspace">
                                    <span class=ORG_SWITCHER_AVATAR aria-hidden="true">{monogram}</span>
                                    <span class=ORG_SWITCHER_META>
                                        <span class=ORG_SWITCHER_LABEL>{label}</span>
                                        <span class=ORG_SWITCHER_HINT>"Workspace"</span>
                                    </span>
                                    <span class=ORG_SWITCHER_CARET aria-hidden="true"></span>
                                </summary>
                                <div class=ORG_SWITCHER_PANEL role="menu">
                                    <p class=ORG_SWITCHER_PANEL_LABEL>"Workspaces"</p>
                                    <ul class=ORG_SWITCHER_LIST>
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
                                                        class=ORG_SWITCHER_ITEM
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
                                                        <span class=ORG_SWITCHER_ITEM_NAME>{name}</span>
                                                        <span class=ORG_SWITCHER_ITEM_META>
                                                            {if is_active { "Active".into() } else { slug_line }}
                                                        </span>
                                                    </button>
                                                </li>
                                            }
                                        }).collect_view()}
                                    </ul>
                                    <div class=ORG_SWITCHER_DIVIDER aria-hidden="true"></div>
                                    <a class=ORG_SWITCHER_LINK href="/organizations" role="menuitem">"Manage workspaces"</a>
                                    <a class=ORG_SWITCHER_LINK href=vault_href role="menuitem">"Secret vault"</a>
                                    <Show when=move || show_settings>
                                        <a class=ORG_SWITCHER_LINK href=settings_href.clone() role="menuitem">
                                            "Workspace settings"
                                        </a>
                                    </Show>
                                </div>
                            </details>
                        }.into_any()
                    }
                    (Some(Ok(_)), _) | (None, _) => view! {
                        <span class=ORG_SWITCHER_FALLBACK>"…"</span>
                    }.into_any(),
                    _ => view! {
                        <a class=ORG_SWITCHER_FALLBACK href="/organizations">"Workspaces"</a>
                    }.into_any(),
                }
            }}
        </div>
    }
}

#[island(lazy)]
pub fn WorkspaceSystemNav() -> impl IntoView {
    let session = browser_load(get_current_session);
    let label_class = with_extra(WS_NAV_LABEL, Some(WS_NAV_LABEL_SECONDARY));

    view! {
        <div class=WS_SYSTEM_NAV>
            {move || match session.get() {
                Some(Ok(session)) if session.authenticated && can_view_system_navigation(&session) => {
                    view! {
                        <p class=label_class.clone()>"System"</p>
                        <a class=WS_NAV_LINK href="/admin/health" data-nav="system" title="Health">
                            {nav_icon("system")}
                            <span class=WS_NAV_TEXT>"Health"</span>
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
        <div class=USER_MENU>
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
                        <details class=USER_MENU_DETAILS data-flyout="user-menu">
                            <summary class=USER_MENU_TRIGGER aria-label="Account menu">
                                <span class=USER_MENU_AVATAR aria-hidden="true">{initial.to_string()}</span>
                                <span class=USER_MENU_META>
                                    <span class=USER_MENU_EMAIL>{email.clone()}</span>
                                    <span class=USER_MENU_HINT>"Account"</span>
                                </span>
                                <span class=USER_MENU_CARET aria-hidden="true"></span>
                            </summary>
                            <div class=USER_MENU_PANEL role="menu">
                                <p class=USER_MENU_PANEL_LABEL>"Account"</p>
                                <a class=USER_MENU_ITEM href="/account/profile" role="menuitem">"Profile"</a>
                                <a class=USER_MENU_ITEM href="/account/password" role="menuitem">"Password"</a>
                                <a class=USER_MENU_ITEM href="/account/mfa" role="menuitem">"Authenticator (MFA)"</a>
                                <a class=USER_MENU_ITEM href="/account/passkeys" role="menuitem">"Passkeys"</a>
                                <a class=USER_MENU_ITEM href="/account/sessions" role="menuitem">"Sessions"</a>
                                <a class=USER_MENU_ITEM href="/account/providers" role="menuitem">"Providers"</a>
                                <div class=USER_MENU_DIVIDER aria-hidden="true"></div>
                                <div class=USER_MENU_LOGOUT>
                                    <LogoutButton />
                                </div>
                            </div>
                        </details>
                    }.into_any()
                }
                Some(Ok(_)) => view! {
                    <a class=USER_MENU_FALLBACK href="/login">"Sign in"</a>
                }.into_any(),
                Some(Err(_)) => view! {
                    <span class=USER_MENU_FALLBACK>"Session unavailable"</span>
                }.into_any(),
                None => view! {
                    <span class=USER_MENU_FALLBACK>"Loading…"</span>
                }.into_any(),
            }}
        </div>
    }
}

/// Root layout: keep shell chrome mounted across navigations within each mode.
///
/// Settings routes nest under `WorkspaceSettingsShell` (outlet only) so they
/// share this workspace rail — primary nav swaps to settings sections by path.
#[component]
pub fn AppLayout() -> impl IntoView {
    let location = use_location();
    let in_workspace = Memo::new(move |_| is_workspace_path(&location.pathname.get()));

    view! {
        {move || {
            if in_workspace.get() {
                view! {
                    <WorkspaceShell>
                        <Outlet />
                    </WorkspaceShell>
                }
                .into_any()
            } else {
                view! {
                    <main class="block min-h-dvh w-full box-border">
                        <Outlet />
                    </main>
                }
                .into_any()
            }
        }}
    }
}

/// Primary sidebar nav chrome (SSR). Capability-filtered **settings links** live in
/// [`WorkspaceSettingsNavLinks`] (island) so `browser_load` runs without hydrating
/// the whole rail (a full-nav island caused tachys hydration panics and left every
/// other island stuck on “Loading…”).
#[component]
fn WorkspacePrimaryNav() -> impl IntoView {
    let location = use_location();
    let settings_mode = Memo::new(move |_| is_workspace_settings_path(&location.pathname.get()));

    view! {
        <nav
            class=WS_NAV
            aria-label=move || {
                if settings_mode.get() {
                    "Workspace settings"
                } else {
                    "Authenticated workspace"
                }
            }
        >
            {move || {
                if settings_mode.get() {
                    view! {
                        <p class=WS_NAV_LABEL>"Settings"</p>
                        <a
                            class=WS_NAV_LINK
                            href="/dashboard"
                            data-nav="overview"
                            title="Back to overview"
                        >
                            {nav_icon("overview")}
                            <span class=WS_NAV_TEXT>"Overview"</span>
                        </a>
                        <WorkspaceSettingsNavLinks />
                    }
                    .into_any()
                } else {
                    // Product rail: auth-gated by the server shell; no client RBAC needed.
                    let product = nav_product_items();
                    view! {
                        {product
                            .iter()
                            .map(|item| {
                                let href = match &item.href {
                                    NavHref::Static(path) => *path,
                                    NavHref::SettingsSection(_) => "/dashboard",
                                };
                                let label = item.label;
                                let data_nav = item.id;
                                let icon = item.icon.unwrap_or("overview");
                                view! {
                                    <a
                                        class=WS_NAV_LINK
                                        href=href
                                        data-nav=data_nav
                                        title=label
                                    >
                                        {nav_icon(icon)}
                                        <span class=WS_NAV_TEXT>{label}</span>
                                    </a>
                                }
                            })
                            .collect_view()}
                        <WorkspaceSystemNav />
                    }
                    .into_any()
                }
            }}
        </nav>
    }
}

/// Island: load org membership capabilities and render settings section links.
///
/// Islands have no Router context — slug comes from `current_browser_pathname()`.
#[island]
pub fn WorkspaceSettingsNavLinks() -> impl IntoView {
    let session = browser_load(get_current_session);
    let orgs = browser_load(list_organizations);

    view! {
        <div data-testid="workspace-settings-nav-links">
            {move || {
                let slug = settings_slug_from_path(&current_browser_pathname());
                let orgs_state = orgs.get();
                let loaded = orgs_state.is_some();
                let load_failed = matches!(&orgs_state, Some(Err(_)));
                let ctx = match (session.get(), orgs_state.as_ref()) {
                    (Some(Ok(view)), Some(Ok(list))) if view.authenticated => {
                        let caps = list
                            .organizations
                            .iter()
                            .find(|org| !slug.is_empty() && org.slug == slug)
                            .map(|org| org.permissions.as_slice())
                            .unwrap_or(&[]);
                        AccessContext::from_permissions(
                            true,
                            caps.iter().map(String::as_str),
                            &view.assurance,
                            view.system_administrator,
                        )
                    }
                    (Some(Ok(view)), _) if view.authenticated => AccessContext::from_permissions(
                        true,
                        std::iter::empty::<&str>(),
                        &view.assurance,
                        view.system_administrator,
                    ),
                    _ => AccessContext::anonymous(),
                };
                let items: Vec<_> = nav_settings_items()
                    .iter()
                    .filter(|item| item.requirement.is_satisfied_by(&ctx))
                    .collect();

                if !loaded {
                    view! {
                        <p class=WS_NAV_LABEL_SECONDARY>"Loading settings…"</p>
                    }
                    .into_any()
                } else if load_failed {
                    view! {
                        <p class=WS_NAV_LABEL_SECONDARY>
                            "Could not load workspace permissions."
                        </p>
                    }
                    .into_any()
                } else if items.is_empty() {
                    view! {
                        <p class=WS_NAV_LABEL_SECONDARY>
                            "No settings available for your role."
                        </p>
                    }
                    .into_any()
                } else {
                    items
                        .into_iter()
                        .map(|item| {
                            let href = match &item.href {
                                NavHref::Static(path) => (*path).to_owned(),
                                NavHref::SettingsSection(segment) => {
                                    if slug.is_empty() {
                                        "/organizations".to_owned()
                                    } else {
                                        format!("/org/{slug}/settings/{segment}")
                                    }
                                }
                            };
                            let label = item.label;
                            let data_nav = item.id;
                            let disabled =
                                slug.is_empty() && matches!(item.href, NavHref::SettingsSection(_));
                            let icon = item.icon;
                            view! {
                                <a
                                    class=WS_NAV_LINK
                                    href=href
                                    data-nav=data_nav
                                    title=label
                                    class:is-disabled=disabled
                                    aria-disabled=disabled
                                >
                                    {icon.map(|kind| nav_icon(kind).into_any())}
                                    <span class=WS_NAV_TEXT>{label}</span>
                                </a>
                            }
                        })
                        .collect_view()
                        .into_any()
                }
            }}
        </div>
    }
}
