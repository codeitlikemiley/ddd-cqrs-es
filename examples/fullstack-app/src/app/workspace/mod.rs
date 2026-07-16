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
use crate::contracts::{OrganizationListResponse, SessionView};
use leptos::prelude::*;
use leptos_router::components::Outlet;
use leptos_router::hooks::use_location;
use server_fn::ServerFnError;
#[cfg(feature = "hydrate")]
use leptos::task::spawn_local;
#[cfg(feature = "hydrate")]
use wasm_bindgen::prelude::*;
#[cfg(feature = "hydrate")]
use web_sys::window;

#[cfg(feature = "hydrate")]
use crate::app::{
    after_island_hydration, bind_user_menu_dismiss, bind_workspace_nav_active,
    init_workspace_chrome_persist, init_workspace_sidebar,
};
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
    WS_NAV_ICON, WS_NAV_LABEL, WS_NAV_LABEL_SECONDARY, WS_NAV_LINK, WS_NAV_SKELETON,
    WS_NAV_SKELETON_ROW, WS_NAV_TEXT, WS_NAV_TOGGLE, WS_RAIL_ICON, WS_RAIL_ICON_BAR,
    WS_RAIL_TOGGLE, WS_SHELL, WS_SIDEBAR, WS_SIDEBAR_CLOSE, WS_SIDEBAR_FOOT, WS_SIDEBAR_TOP,
    WS_SYSTEM_NAV, WS_TOPBAR, WS_TOPBAR_BRAND, WS_TOPBAR_ORG, WS_TOPBAR_PAGE, WS_TOPBAR_TITLE,
    with_extra,
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
                // Primary nav can swap product ↔ settings; foot chrome islands
                // stay outside that branch so islands-router preserves them.
                <div id="workspace-primary-nav" data-workspace-region="primary-nav">
                    <WorkspacePrimaryNav />
                </div>
                <div
                    class=WS_SIDEBAR_FOOT
                    id="workspace-chrome-foot"
                    data-workspace-region="chrome-foot"
                    data-chrome-persist="true"
                >
                    <WorkspaceOrgSwitcher />
                    <WorkspaceUserMenu />
                    <ThemeToggle />
                </div>
            </aside>
            <div class=WS_MAIN>
                <header class=WS_TOPBAR id="workspace-topbar" data-workspace-region="topbar">
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
                    <div class=WS_TOPBAR_TITLE id="workspace-topbar-title">
                        <span class=WS_TOPBAR_PAGE>{move || topbar_title.get()}</span>
                    </div>
                    <div
                        class=WS_TOPBAR_ORG
                        data-org-placement="top"
                        data-chrome-persist="true"
                    >
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
                // Soft-nav swaps page bodies here; chrome islands above are preserved.
                <div
                    class=WS_CONTENT
                    id="workspace-content"
                    data-workspace-region="content"
                    data-testid="workspace-content"
                >
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
/// Also installs content-only soft-nav so chrome islands stay mounted (Option B).
#[island]
pub fn WorkspaceSidebarControls() -> impl IntoView {
    Effect::new(move |_| {
        #[cfg(feature = "hydrate")]
        {
            init_workspace_sidebar();
            init_workspace_chrome_persist();
        }
    });
    view! { <span class=WS_HIDDEN_MARKER aria-hidden="true"></span> }
}

/// Top-bar / sidebar workspace switcher: select org, jump to vault, create workspace.
///
/// Click-away / Escape dismiss is shared with the account flyout via
/// `bindUserMenuDismiss` (also installed from workspace sidebar init).
///
/// Cache-first (same snapshot as settings nav + account menu): islands remount on
/// soft navigations — never flash a pulse skeleton when we already know the tenant.
#[island]
pub fn WorkspaceOrgSwitcher() -> impl IntoView {
    let (snapshot, set_snapshot) =
        signal(None::<Result<WorkspaceChromeSnapshot, ServerFnError>>);
    let select_action = ServerAction::<SelectOrganization>::new();
    let select_pending = select_action.pending();

    #[cfg(feature = "hydrate")]
    Effect::new(move |_| {
        bind_user_menu_dismiss();
        spawn_local(async move {
            let _ = after_island_hydration().await;
            // Instant restore so route changes keep the trigger visible.
            if let Some(cached) = read_workspace_chrome_cache() {
                set_snapshot.set(Some(Ok(cached)));
                mark_active_nav(&current_browser_pathname());
            }
            if let Some(fresh) = refresh_workspace_chrome_cache().await {
                set_snapshot.set(Some(Ok(fresh)));
                mark_active_nav(&current_browser_pathname());
            } else if snapshot.get_untracked().is_none() {
                // First paint, no cache, network failed — leave stable shell.
            }
        });
    });

    Effect::new(move |_| {
        if matches!(select_action.value().get(), Some(Ok(_))) {
            #[cfg(feature = "hydrate")]
            {
                // Tenant change invalidates chrome labels; full reload is intentional.
                clear_workspace_chrome_cache();
                if let Some(window) = window() {
                    let _ = window.location().reload();
                }
            }
        }
    });

    // Client path focus for flyout links after paint / snapshot swap.
    Effect::new(move |_| {
        let _ = snapshot.get();
        #[cfg(feature = "hydrate")]
        {
            mark_active_nav(&current_browser_pathname());
        }
    });

    view! {
        <div class=ORG_SWITCHER data-testid="workspace-org-switcher">
            {move || match snapshot.get() {
                Some(Ok(snap)) if snap.session.authenticated => {
                    render_org_switcher(&snap, select_action, select_pending)
                }
                Some(Ok(_)) => view! {
                    <a class=ORG_SWITCHER_FALLBACK href="/organizations">"Workspaces"</a>
                }
                .into_any(),
                Some(Err(_)) => view! {
                    <a class=ORG_SWITCHER_FALLBACK href="/organizations">"Workspaces"</a>
                }
                .into_any(),
                // SSR + first client frame: stable shell (ThemeToggle-style), never pulse.
                None => org_switcher_stable_shell().into_any(),
            }}
        </div>
    }
}

/// Non-loading chrome for SSR / pre-cache frames (no animate-pulse).
fn org_switcher_stable_shell() -> impl IntoView {
    view! {
        <div class=ORG_SWITCHER_DETAILS aria-hidden="true">
            <div class=ORG_SWITCHER_TRIGGER>
                <span class=ORG_SWITCHER_AVATAR aria-hidden="true">"W"</span>
                <span class=ORG_SWITCHER_META>
                    <span class=ORG_SWITCHER_LABEL>"Workspace"</span>
                    <span class=ORG_SWITCHER_HINT>"Workspace"</span>
                </span>
                <span class=ORG_SWITCHER_CARET aria-hidden="true"></span>
            </div>
        </div>
    }
}

fn render_org_switcher(
    snap: &WorkspaceChromeSnapshot,
    select_action: ServerAction<SelectOrganization>,
    select_pending: Memo<bool>,
) -> AnyView {
    let sess = &snap.session;
    let list = &snap.organizations;
    let active_id = sess.tenant_id.clone().filter(|s| !s.trim().is_empty());
    let active = active_id.as_ref().and_then(|id| {
        list.organizations
            .iter()
            .find(|o| o.organization_id == *id)
            .cloned()
    });
    let label = active.as_ref().map(|o| o.name.clone()).unwrap_or_else(|| {
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
                    {orgs_for_list
                        .into_iter()
                        .map(|org| {
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
                                            if is_active {
                                                return;
                                            }
                                            select_action.dispatch(SelectOrganization {
                                                organization_id: id_select.clone(),
                                            });
                                        }
                                    >
                                        <span class=ORG_SWITCHER_ITEM_NAME>{name}</span>
                                        <span class=ORG_SWITCHER_ITEM_META>
                                            {if is_active {
                                                "Active".into()
                                            } else {
                                                slug_line
                                            }}
                                        </span>
                                    </button>
                                </li>
                            }
                        })
                        .collect_view()}
                </ul>
                <div class=ORG_SWITCHER_DIVIDER aria-hidden="true"></div>
                <a
                    class=ORG_SWITCHER_LINK
                    href="/organizations"
                    role="menuitem"
                    data-nav="organizations"
                >
                    "Manage workspaces"
                </a>
                <a
                    class=ORG_SWITCHER_LINK
                    href=vault_href
                    role="menuitem"
                    data-nav="account-vault"
                >
                    "Secret vault"
                </a>
                <Show when=move || show_settings>
                    <a
                        class=ORG_SWITCHER_LINK
                        href=settings_href.clone()
                        role="menuitem"
                        data-nav="settings-general"
                    >
                        "Workspace settings"
                    </a>
                </Show>
            </div>
        </details>
    }
    .into_any()
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
///
/// Cache-first like the org switcher: route changes remount this island but must
/// not re-flash a skeleton. Flyout link focus is client-side via `data-nav`.
#[island]
pub fn WorkspaceUserMenu() -> impl IntoView {
    let (snapshot, set_snapshot) =
        signal(None::<Result<WorkspaceChromeSnapshot, ServerFnError>>);

    #[cfg(feature = "hydrate")]
    Effect::new(move |_| {
        bind_user_menu_dismiss();
        spawn_local(async move {
            let _ = after_island_hydration().await;
            if let Some(cached) = read_workspace_chrome_cache() {
                set_snapshot.set(Some(Ok(cached)));
                mark_active_nav(&current_browser_pathname());
            }
            if let Some(fresh) = refresh_workspace_chrome_cache().await {
                set_snapshot.set(Some(Ok(fresh)));
                mark_active_nav(&current_browser_pathname());
            }
        });
    });

    Effect::new(move |_| {
        let _ = snapshot.get();
        #[cfg(feature = "hydrate")]
        {
            mark_active_nav(&current_browser_pathname());
        }
    });

    view! {
        <div class=USER_MENU data-testid="workspace-user-menu">
            {move || match snapshot.get() {
                Some(Ok(snap)) if snap.session.authenticated => {
                    render_user_menu(&snap.session)
                }
                Some(Ok(_)) => view! {
                    <a class=USER_MENU_FALLBACK href="/login">"Sign in"</a>
                }
                .into_any(),
                Some(Err(_)) => view! {
                    <span class=USER_MENU_FALLBACK>"Session unavailable"</span>
                }
                .into_any(),
                None => user_menu_stable_shell().into_any(),
            }}
        </div>
    }
}

fn user_menu_stable_shell() -> impl IntoView {
    view! {
        <div class=USER_MENU_DETAILS aria-hidden="true">
            <div class=USER_MENU_TRIGGER>
                <span class=USER_MENU_AVATAR aria-hidden="true">"A"</span>
                <span class=USER_MENU_META>
                    <span class=USER_MENU_EMAIL>"Account"</span>
                    <span class=USER_MENU_HINT>"Account"</span>
                </span>
                <span class=USER_MENU_CARET aria-hidden="true"></span>
            </div>
        </div>
    }
}

fn render_user_menu(session: &SessionView) -> AnyView {
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
                <a
                    class=USER_MENU_ITEM
                    href="/account/profile"
                    role="menuitem"
                    data-nav="account-profile"
                >
                    "Profile"
                </a>
                <a
                    class=USER_MENU_ITEM
                    href="/account/password"
                    role="menuitem"
                    data-nav="account-password"
                >
                    "Password"
                </a>
                <a
                    class=USER_MENU_ITEM
                    href="/account/mfa"
                    role="menuitem"
                    data-nav="account-mfa"
                >
                    "Authenticator (MFA)"
                </a>
                <a
                    class=USER_MENU_ITEM
                    href="/account/passkeys"
                    role="menuitem"
                    data-nav="account-passkeys"
                >
                    "Passkeys"
                </a>
                <a
                    class=USER_MENU_ITEM
                    href="/account/sessions"
                    role="menuitem"
                    data-nav="account-sessions"
                >
                    "Sessions"
                </a>
                <a
                    class=USER_MENU_ITEM
                    href="/account/providers"
                    role="menuitem"
                    data-nav="account-providers"
                >
                    "Providers"
                </a>
                <div class=USER_MENU_DIVIDER aria-hidden="true"></div>
                <div class=USER_MENU_LOGOUT>
                    <LogoutButton />
                </div>
            </div>
        </details>
    }
    .into_any()
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
                    // Pass slug from Router (SSR-correct). Island must not rely on
                    // current_browser_pathname() for first paint — on SSR it is "/"
                    // which produced disabled /organizations links (pointer-events:none).
                    let settings_slug =
                        settings_slug_from_path(&location.pathname.get());
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
                        <WorkspaceSettingsNavLinks slug=settings_slug />
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

/// Shared client chrome cache: session + org membership.
///
/// Used by org switcher, account menu, and settings nav so islands can remount
/// on soft navigations without re-flashing loaders (ThemeToggle-style stability).
#[derive(Clone)]
struct WorkspaceChromeSnapshot {
    session: SessionView,
    organizations: OrganizationListResponse,
}

/// Back-compat alias for settings nav rendering.
type SettingsNavSnapshot = WorkspaceChromeSnapshot;

#[cfg(feature = "hydrate")]
const WORKSPACE_CHROME_CACHE_KEY: &str = "workspace-chrome-v1";

#[cfg(feature = "hydrate")]
thread_local! {
    static WORKSPACE_CHROME_MEMORY: std::cell::RefCell<Option<WorkspaceChromeSnapshot>> =
        const { std::cell::RefCell::new(None) };
    /// Coalesce parallel refreshes from multiple remounting islands.
    static WORKSPACE_CHROME_REFRESHING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(feature = "hydrate")]
fn read_workspace_chrome_cache() -> Option<WorkspaceChromeSnapshot> {
    if let Some(hit) = WORKSPACE_CHROME_MEMORY.with(|cell| cell.borrow().clone()) {
        return Some(hit);
    }
    let window = window()?;
    let storage = window.session_storage().ok().flatten()?;
    let raw = storage.get_item(WORKSPACE_CHROME_CACHE_KEY).ok().flatten()?;
    let (session, organizations): (SessionView, OrganizationListResponse) =
        serde_json::from_str(&raw).ok()?;
    let snap = WorkspaceChromeSnapshot {
        session,
        organizations,
    };
    WORKSPACE_CHROME_MEMORY.with(|cell| *cell.borrow_mut() = Some(snap.clone()));
    Some(snap)
}

#[cfg(feature = "hydrate")]
fn write_workspace_chrome_cache(snap: &WorkspaceChromeSnapshot) {
    WORKSPACE_CHROME_MEMORY.with(|cell| *cell.borrow_mut() = Some(snap.clone()));
    if let Some(window) = window() {
        if let Ok(Some(storage)) = window.session_storage() {
            if let Ok(raw) = serde_json::to_string(&(&snap.session, &snap.organizations)) {
                let _ = storage.set_item(WORKSPACE_CHROME_CACHE_KEY, &raw);
            }
        }
    }
}

#[cfg(feature = "hydrate")]
fn clear_workspace_chrome_cache() {
    WORKSPACE_CHROME_MEMORY.with(|cell| *cell.borrow_mut() = None);
    if let Some(window) = window() {
        if let Ok(Some(storage)) = window.session_storage() {
            let _ = storage.remove_item(WORKSPACE_CHROME_CACHE_KEY);
        }
    }
}

/// Fetch session+orgs once and populate the shared chrome cache.
/// Returns `None` on failure (callers keep prior cache / stable shell).
///
/// Parallel island remounts (sidebar org switcher + topbar duplicate + user menu)
/// share one in-flight refresh so we do not stampede `/api/ui/*`.
#[cfg(feature = "hydrate")]
async fn refresh_workspace_chrome_cache() -> Option<WorkspaceChromeSnapshot> {
    // Wait for an in-flight refresh (poll memory).
    if WORKSPACE_CHROME_REFRESHING.get() {
        for _ in 0..100 {
            chrome_sleep_ms(20).await;
            if !WORKSPACE_CHROME_REFRESHING.get() {
                break;
            }
        }
        return read_workspace_chrome_cache();
    }

    WORKSPACE_CHROME_REFRESHING.set(true);
    let result = async {
        let session = get_current_session().await.ok()?;
        let organizations = list_organizations().await.ok()?;
        if !session.authenticated {
            return None;
        }
        let snap = WorkspaceChromeSnapshot {
            session,
            organizations,
        };
        write_workspace_chrome_cache(&snap);
        Some(snap)
    }
    .await;
    WORKSPACE_CHROME_REFRESHING.set(false);
    result
}

#[cfg(feature = "hydrate")]
async fn chrome_sleep_ms(ms: i32) {
    use js_sys::Promise;
    use wasm_bindgen_futures::JsFuture;
    let promise = Promise::new(&mut |resolve, _reject| {
        if let Some(window) = web_sys::window() {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
        } else {
            let _ = resolve.call0(&JsValue::NULL);
        }
    });
    let _ = JsFuture::from(promise).await;
}

#[cfg(feature = "hydrate")]
fn read_settings_nav_cache() -> Option<SettingsNavSnapshot> {
    read_workspace_chrome_cache()
}

#[cfg(feature = "hydrate")]
fn write_settings_nav_cache(snap: &SettingsNavSnapshot) {
    write_workspace_chrome_cache(snap);
}

fn settings_nav_skeleton() -> AnyView {
    view! {
        <div class=WS_NAV_SKELETON aria-busy="true" aria-label="Loading settings navigation">
            <span class=WS_NAV_SKELETON_ROW></span>
            <span class=WS_NAV_SKELETON_ROW></span>
            <span class=WS_NAV_SKELETON_ROW></span>
            <span class=WS_NAV_SKELETON_ROW></span>
            <span class=WS_NAV_SKELETON_ROW></span>
            <span class=WS_NAV_SKELETON_ROW></span>
        </div>
    }
    .into_any()
}

fn render_settings_nav_items_unfiltered(slug: &str) -> AnyView {
    render_settings_nav_item_list(slug, nav_settings_items().iter().collect())
}

fn render_settings_nav_items(slug: &str, ctx: &AccessContext) -> AnyView {
    let items: Vec<_> = nav_settings_items()
        .iter()
        .filter(|item| item.requirement.is_satisfied_by(ctx))
        .collect();
    if items.is_empty() {
        return view! {
            <p class=WS_NAV_LABEL_SECONDARY>
                "No settings available for your role."
            </p>
        }
        .into_any();
    }
    render_settings_nav_item_list(slug, items)
}

fn render_settings_nav_item_list(
    slug: &str,
    items: Vec<&crate::access::NavItem>,
) -> AnyView {
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
            let disabled = slug.is_empty() && matches!(item.href, NavHref::SettingsSection(_));
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

/// Island: load org membership capabilities and render settings section links.
///
/// `slug` comes from the parent Router path on SSR so hrefs are correct in HTML
/// (never `current_browser_pathname()` on first paint — SSR returns "/" and used
/// to disable every link with `pointer-events: none`).
///
/// After hydrate: keep slug live for SPA hops, restore cache, refresh RBAC, and
/// re-run `mark_active_nav` so the focused section highlights.
#[island]
pub fn WorkspaceSettingsNavLinks(slug: String) -> impl IntoView {
    let slug = RwSignal::new(slug);
    // None = optimistic full catalog (SSR-safe). Some = role-filtered snapshot.
    let (snapshot, set_snapshot) = signal(None::<Result<SettingsNavSnapshot, ServerFnError>>);

    #[cfg(feature = "hydrate")]
    Effect::new(move |_| {
        // Prefer live browser path after mount (SPA / soft navigations).
        let live = settings_slug_from_path(&current_browser_pathname());
        if !live.is_empty() {
            slug.set(live);
        }
        use wasm_bindgen::JsCast;
        use wasm_bindgen::closure::Closure;
        let on_nav = Closure::wrap(Box::new(move |_event: web_sys::Event| {
            let live = settings_slug_from_path(&current_browser_pathname());
            if !live.is_empty() {
                slug.set(live);
            }
            mark_active_nav(&current_browser_pathname());
        }) as Box<dyn FnMut(_)>);
        if let Some(window) = window() {
            let _ = window.add_event_listener_with_callback(
                "workspace-nav-mark",
                on_nav.as_ref().unchecked_ref(),
            );
            on_nav.forget();
        }

        spawn_local(async move {
            let _ = after_island_hydration().await;
            if let Some(cached) = read_workspace_chrome_cache() {
                set_snapshot.set(Some(Ok(cached)));
            }
            mark_active_nav(&current_browser_pathname());
            if let Some(fresh) = refresh_workspace_chrome_cache().await {
                set_snapshot.set(Some(Ok(fresh)));
                mark_active_nav(&current_browser_pathname());
            } else if snapshot.get_untracked().is_none()
                && read_workspace_chrome_cache().is_none()
            {
                // Leave optimistic unfiltered catalog (None) rather than skeleton.
            }
        });
    });
    #[cfg(not(feature = "hydrate"))]
    {
        let _ = &set_snapshot;
    }

    // Re-highlight when slug or filtered links change.
    Effect::new(move |_| {
        let _ = slug.get();
        let _ = snapshot.get();
        #[cfg(feature = "hydrate")]
        {
            mark_active_nav(&current_browser_pathname());
        }
    });

    view! {
        <div data-testid="workspace-settings-nav-links">
            {move || {
                let slug = slug.get();
                match snapshot.get() {
                    // Optimistic / SSR: full catalog with the Router-provided slug.
                    None => render_settings_nav_items_unfiltered(&slug),
                    Some(Err(_)) => settings_nav_skeleton(),
                    Some(Ok(snap)) => {
                        let caps = snap
                            .organizations
                            .organizations
                            .iter()
                            .find(|org| !slug.is_empty() && org.slug == slug)
                            .map(|org| org.permissions.as_slice())
                            .unwrap_or(&[]);
                        let ctx = AccessContext::from_permissions(
                            snap.session.authenticated,
                            caps.iter().map(String::as_str),
                            &snap.session.assurance,
                            snap.session.system_administrator,
                        );
                        render_settings_nav_items(&slug, &ctx)
                    }
                }
            }}
        </div>
    }
}
