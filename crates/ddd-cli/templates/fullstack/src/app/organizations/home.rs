//! Workspaces switcher home (`/organizations`).

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use super::create_modal::CreateOrganizationModal;
use crate::app::helpers::{
    action_result_text, org_monogram, org_tone_index, server_error_text, short_id_label,
};
use crate::app::{
    ListOrganizations, SelectOrganization, browser_load, get_current_session, list_organizations,
    select_organization,
};
use crate::ui::classes::{
    BANNER_ERROR, BTN_PRIMARY, BTN_SECONDARY, MONO_VALUE, ORG_BADGE, ORG_BADGE_ACTIVE, ORG_EMPTY,
    ORG_EMPTY_MARK, ORG_EMPTY_P, ORG_EMPTY_TITLE, ORG_KICKER, ORG_LIST, ORG_LIST_PANEL, ORG_PAGE,
    ORG_ROW_ACTIONS, ORG_ROW_MAIN, ORG_ROW_META, ORG_ROW_TITLE, ORG_SKELETON, ORG_STATUS,
    ORG_TOOLBAR, ORG_TOOLBAR_COPY, ORG_TOOLBAR_SUB, ORG_TOOLBAR_TITLE, org_avatar_class,
    org_row_class, with_extra,
};
use leptos::prelude::*;

#[island]
pub fn OrganizationsHome() -> impl IntoView {
    let organizations = browser_load(list_organizations);
    let session = browser_load(get_current_session);
    let select_action = ServerAction::<SelectOrganization>::new();
    let select_pending = select_action.pending();
    let select_value = select_action.value();
    let (create_open, set_create_open) = signal(false);

    Effect::new(move |_| {
        if matches!(select_value.get(), Some(Ok(_))) {
            #[cfg(feature = "hydrate")]
            {
                if let Some(window) = web_sys::window() {
                    let _ = window.location().reload();
                }
            }
        }
    });

    view! {
        <div class=ORG_PAGE>
            <header class=ORG_TOOLBAR>
                <div class=ORG_TOOLBAR_COPY>
                    <p class=ORG_KICKER>"Tenancy"</p>
                    <h2 class=ORG_TOOLBAR_TITLE>"Your workspaces"</h2>
                    <p class=ORG_TOOLBAR_SUB>
                        "The first workspace is the default after sign-in. Select another to switch the active tenant."
                    </p>
                </div>
                <button
                    type="button"
                    class=BTN_PRIMARY
                    on:click=move |_| set_create_open.set(true)
                >
                    "New workspace"
                </button>
            </header>

            <Show when=move || select_value.get().is_some_and(|result| result.is_err())>
                <p class=BANNER_ERROR>{move || action_result_text(select_value.get())}</p>
            </Show>

            <section class=ORG_LIST_PANEL aria-label="Workspace list">
                {move || {
                    let active_tenant = session
                        .get()
                        .and_then(Result::ok)
                        .and_then(|s| s.tenant_id)
                        .filter(|value| !value.trim().is_empty());
                    match organizations.get() {
                        Some(Ok(response)) if response.organizations.is_empty() => view! {
                            <div class=ORG_EMPTY>
                                <div class=ORG_EMPTY_MARK aria-hidden="true">"W"</div>
                                <h3 class=ORG_EMPTY_TITLE>"No workspaces yet"</h3>
                                <p class=ORG_EMPTY_P>"Create your first workspace to invite teammates and manage roles."</p>
                                <button
                                    type="button"
                                    class=BTN_PRIMARY
                                    on:click=move |_| set_create_open.set(true)
                                >
                                    "Create workspace"
                                </button>
                            </div>
                        }.into_any(),
                        Some(Ok(response)) => view! {
                            <ul class=ORG_LIST>
                                <For
                                    each=move || response.organizations.clone()
                                    key=|organization| organization.organization_id.clone()
                                    children=move |organization| {
                                        let organization_id = organization.organization_id.clone();
                                        let select_id = organization_id.clone();
                                        let org_slug = organization.slug.clone();
                                        let is_active = active_tenant
                                            .as_ref()
                                            .is_some_and(|id| id == &organization_id);
                                        let monogram = org_monogram(&organization.name);
                                        let tone = org_tone_index(&organization.name);
                                        let avatar_class = org_avatar_class(tone);
                                        let row_class = org_row_class(is_active);
                                        let role = organization.current_user_role.clone();
                                        let status = organization.status.clone();
                                        let vault_href = if org_slug.is_empty() {
                                            "/account/vault".to_owned()
                                        } else {
                                            format!("/org/{org_slug}/vault")
                                        };
                                        let settings_href = if org_slug.is_empty() {
                                            "/organizations/settings".to_owned()
                                        } else {
                                            format!("/org/{org_slug}/settings/general")
                                        };
                                        let action = if is_active {
                                            view! {
                                                <a class=BTN_SECONDARY href=vault_href.clone()>"Vault"</a>
                                                <a class=BTN_SECONDARY href=settings_href.clone()>
                                                    "Settings"
                                                </a>
                                            }
                                            .into_any()
                                        } else {
                                            view! {
                                                <a class=BTN_SECONDARY href=vault_href.clone()>"Vault"</a>
                                                <button
                                                    type="button"
                                                    class=BTN_SECONDARY
                                                    disabled=move || select_pending.get()
                                                    on:click=move |_| {
                                                        select_action.dispatch(SelectOrganization {
                                                            organization_id: select_id.clone(),
                                                        });
                                                    }
                                                >
                                                    "Select"
                                                </button>
                                            }
                                            .into_any()
                                        };
                                        view! {
                                            <li class=row_class>
                                                <div
                                                    class=avatar_class
                                                    aria-hidden="true"
                                                >
                                                    {monogram}
                                                </div>
                                                <div class=ORG_ROW_MAIN>
                                                    <div class=ORG_ROW_TITLE>
                                                        <strong>{organization.name.clone()}</strong>
                                                        {if is_active {
                                                            view! {
                                                                <span class=ORG_BADGE_ACTIVE>"Active"</span>
                                                            }
                                                            .into_any()
                                                        } else {
                                                            view! {}.into_any()
                                                        }}
                                                    </div>
                                                    <div class=ORG_ROW_META>
                                                        <span class=ORG_BADGE>{role}</span>
                                                        <span class=ORG_STATUS>{status}</span>
                                                        {if org_slug.is_empty() {
                                                            view! {}.into_any()
                                                        } else {
                                                            let slug_class = with_extra(ORG_STATUS, Some(MONO_VALUE));
                                                            view! {
                                                                <span class=slug_class>{format!("/{org_slug}")}</span>
                                                            }
                                                            .into_any()
                                                        }}
                                                    </div>
                                                </div>
                                                <div class=ORG_ROW_ACTIONS>
                                                    {action}
                                                </div>
                                            </li>
                                        }
                                    }
                                />
                            </ul>
                        }.into_any(),
                        Some(Err(error)) => view! {
                            <p class=BANNER_ERROR>{server_error_text(error)}</p>
                        }.into_any(),
                        None => view! {
                            <div class=ORG_SKELETON aria-busy="true">
                                <span></span><span></span><span></span>
                            </div>
                        }.into_any(),
                    }
                }}
                <Show when=move || {
                    select_value.get().is_some_and(|result| result.is_err())
                }>
                    <p class=BANNER_ERROR>{move || action_result_text(select_value.get())}</p>
                </Show>
            </section>

            <CreateOrganizationModal open=create_open set_open=set_create_open />
        </div>
    }
}
