//! Workspaces switcher home (`/organizations`).

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use super::create_modal::CreateOrganizationModal;
use crate::app::helpers::{
    action_result_text, has_permission, org_monogram, org_tone_index, server_error_text,
    short_id_label,
};
use crate::app::{
    ListOrganizations, SelectOrganization, browser_load, get_current_session, list_organizations,
    select_organization,
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
        <div class="orgs-page">
            <header class="orgs-toolbar">
                <div class="orgs-toolbar-copy">
                    <p class="dash-eyebrow">"Tenancy"</p>
                    <h2 class="orgs-toolbar-title">"Your workspaces"</h2>
                    <p class="orgs-toolbar-sub">
                        "The first workspace is the default after sign-in. Select another to switch the active tenant."
                    </p>
                </div>
                <button
                    type="button"
                    class="primary-button"
                    on:click=move |_| set_create_open.set(true)
                >
                    "New organization"
                </button>
            </header>

            <Show when=move || select_value.get().is_some_and(|result| result.is_err())>
                <p class="error-banner">{move || action_result_text(select_value.get())}</p>
            </Show>

            {move || match session.get() {
                Some(Ok(session)) if session.authenticated => {
                    let can_settings = has_permission(&session, "organization.update");
                    let can_members = has_permission(&session, "member.view");
                    let can_roles = has_permission(&session, "role.view");
                    let can_audit = has_permission(&session, "audit.view");
                    let active_tenant = session
                        .tenant_id
                        .clone()
                        .filter(|value| !value.trim().is_empty());
                    if can_settings || can_members || can_roles || can_audit {
                        view! {
                            <nav class="orgs-context-nav" aria-label="Selected organization">
                                <div class="orgs-context-label">
                                    <span class="dash-metric-dot dash-metric-dot-ok" aria-hidden="true"></span>
                                    <span>
                                        {match active_tenant.as_ref() {
                                            Some(id) => format!("Active · {}", short_id_label(id)),
                                            None => "Select an organization to unlock management".to_owned(),
                                        }}
                                    </span>
                                </div>
                                <div class="orgs-context-links">
                                    <a class="orgs-context-link" href="/account/vault">"Vault"</a>
                                    <Show when=move || can_settings>
                                        <a class="orgs-context-link" href="/organizations/settings">"Settings"</a>
                                    </Show>
                                    <Show when=move || can_members>
                                        <a class="orgs-context-link" href="/organizations/members">"Members"</a>
                                        <a class="orgs-context-link" href="/organizations/invitations">"Invitations"</a>
                                    </Show>
                                    <Show when=move || can_roles>
                                        <a class="orgs-context-link" href="/organizations/roles">"Roles"</a>
                                        <a class="orgs-context-link" href="/organizations/permissions">"Permissions"</a>
                                    </Show>
                                    <Show when=move || can_audit>
                                        <a class="orgs-context-link" href="/organizations/audit">"Audit"</a>
                                    </Show>
                                </div>
                            </nav>
                        }.into_any()
                    } else {
                        view! {}.into_any()
                    }
                }
                _ => view! {}.into_any(),
            }}

            <section class="orgs-list-panel" aria-label="Organization list">
                {move || {
                    let active_tenant = session
                        .get()
                        .and_then(Result::ok)
                        .and_then(|s| s.tenant_id)
                        .filter(|value| !value.trim().is_empty());
                    match organizations.get() {
                        Some(Ok(response)) if response.organizations.is_empty() => view! {
                            <div class="orgs-empty">
                                <div class="orgs-empty-mark" aria-hidden="true">"O"</div>
                                <h3>"No organizations yet"</h3>
                                <p>"Create your first workspace to invite teammates and manage roles."</p>
                                <button
                                    type="button"
                                    class="primary-button"
                                    on:click=move |_| set_create_open.set(true)
                                >
                                    "Create organization"
                                </button>
                            </div>
                        }.into_any(),
                        Some(Ok(response)) => view! {
                            <ul class="orgs-list">
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
                                        let role = organization.current_user_role.clone();
                                        let status = organization.status.clone();
                                        let vault_href = if org_slug.is_empty() {
                                            "/account/vault".to_owned()
                                        } else {
                                            format!("/org/{org_slug}/vault")
                                        };
                                        let action = if is_active {
                                            view! {
                                                <a class="secondary-button" href=vault_href.clone()>"Vault"</a>
                                                <a class="secondary-button" href="/organizations/settings">
                                                    "Open"
                                                </a>
                                            }
                                            .into_any()
                                        } else {
                                            view! {
                                                <a class="secondary-button" href=vault_href.clone()>"Vault"</a>
                                                <button
                                                    type="button"
                                                    class="secondary-button"
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
                                            <li
                                                class="orgs-row"
                                                class:is-active=is_active
                                            >
                                                <div
                                                    class="orgs-avatar"
                                                    data-tone=tone.to_string()
                                                    aria-hidden="true"
                                                >
                                                    {monogram}
                                                </div>
                                                <div class="orgs-row-main">
                                                    <div class="orgs-row-title">
                                                        <strong>{organization.name.clone()}</strong>
                                                        {if is_active {
                                                            view! {
                                                                <span class="orgs-badge orgs-badge-active">"Active"</span>
                                                            }
                                                            .into_any()
                                                        } else {
                                                            view! {}.into_any()
                                                        }}
                                                    </div>
                                                    <div class="orgs-row-meta">
                                                        <span class="orgs-badge">{role}</span>
                                                        <span class="orgs-status">{status}</span>
                                                    </div>
                                                </div>
                                                <div class="orgs-row-actions">
                                                    {action}
                                                </div>
                                            </li>
                                        }
                                    }
                                />
                            </ul>
                        }.into_any(),
                        Some(Err(error)) => view! {
                            <p class="error-banner">{server_error_text(error)}</p>
                        }.into_any(),
                        None => view! {
                            <div class="orgs-skeleton" aria-busy="true">
                                <span></span><span></span><span></span>
                            </div>
                        }.into_any(),
                    }
                }}
                <Show when=move || {
                    select_value.get().is_some_and(|result| result.is_err())
                }>
                    <p class="error-banner">{move || action_result_text(select_value.get())}</p>
                </Show>
            </section>

            <CreateOrganizationModal open=create_open set_open=set_create_open />
        </div>
    }
}
