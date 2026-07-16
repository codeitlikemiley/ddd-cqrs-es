//! Legacy `/organizations/*` management pages.
//!
//! Kept intact until PR2 redirects these routes into slug-scoped workspace settings.

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use super::links::OrganizationLinks;
use crate::app::auth::SessionSummary;
use crate::app::helpers::{action_result_text, server_error_text};
use crate::app::{
    InviteCurrentOrganizationMember, ListCurrentOrganizationAudit,
    ListCurrentOrganizationInvitations, ListCurrentOrganizationMembers,
    ListCurrentOrganizationRoles, UpsertCurrentOrganizationRole, browser_load,
    invite_current_organization_member, list_current_organization_audit,
    list_current_organization_invitations, list_current_organization_members,
    list_current_organization_roles, upsert_current_organization_role,
};
use crate::ui::page_shell;
use leptos::prelude::*;

#[component]
pub fn OrganizationSettingsPage() -> impl IntoView {
    page_shell(
        "Organization settings",
        "The selected tenant comes from the verified session, never from an untrusted form alone.",
        view! { <SessionSummary /> <OrganizationLinks /> },
    )
}

#[island(lazy)]
pub fn OrganizationMembersPage() -> impl IntoView {
    let members = browser_load(list_current_organization_members);
    page_shell(
        "Members",
        "Review active, blocked, and removed organization memberships.",
        view! {
            <section class="panel">
                <h2>"Memberships"</h2>
                <div class="client-data-slot">
                    {move || match members.get() {
                        Some(Ok(response)) => view! {
                            <dl class="kv">
                                <For
                                    each=move || response.memberships.clone()
                                    key=|membership| membership.user_id.clone()
                                    children=move |membership| view! {
                                        <dt>{membership.primary_email}</dt>
                                        <dd>{format!("{} / {}", membership.role_id, membership.status)}</dd>
                                    }
                                />
                            </dl>
                        }.into_any(),
                        Some(Err(error)) => view! { <p class="error-banner">{server_error_text(error)}</p> }.into_any(),
                        None => view! { <p class="result-line">"Loading members"</p> }.into_any(),
                    }}
                </div>
            </section>
            <OrganizationLinks />
        },
    )
}

#[island(lazy)]
pub fn OrganizationInvitationsPage() -> impl IntoView {
    let invitations = browser_load(list_current_organization_invitations);
    let invite_action = ServerAction::<InviteCurrentOrganizationMember>::new();
    let invite_pending = invite_action.pending();
    let invite_value = invite_action.value();
    let (email, set_email) = signal(String::new());
    let (role_id, set_role_id) = signal("member".to_owned());
    page_shell(
        "Invitations",
        "One-time invitation values are mailed and only their hashes are persisted.",
        view! {
            <section class="panel">
                <h2>"Invite member"</h2>
                <label><span>"Email"</span><input type="email" prop:value=move || email.get() on:input=move |event| set_email.set(event_target_value(&event)) /></label>
                <label><span>"Role"</span><input type="text" prop:value=move || role_id.get() on:input=move |event| set_role_id.set(event_target_value(&event)) /></label>
                <button type="button" class="primary-button" disabled=move || invite_pending.get() on:click=move |_| {
                    invite_action.dispatch(InviteCurrentOrganizationMember {
                        email: email.get_untracked(),
                        role_id: role_id.get_untracked(),
                    });
                }>"Send invitation"</button>
                <Show when=move || invite_value.get().is_some()><p class="result-line">{move || action_result_text(invite_value.get())}</p></Show>
            </section>
            <section class="panel">
                <h2>"Invitation status"</h2>
                <div class="client-data-slot">
                    {move || match invitations.get() {
                        Some(Ok(response)) => view! {
                            <dl class="kv"><For each=move || response.invitations.clone() key=|invitation| invitation.invitation_id.clone() children=move |invitation| view! {
                                <dt>{invitation.email}</dt><dd>{format!("{} / {}", invitation.role_id, invitation.status)}</dd>
                            } /></dl>
                        }.into_any(),
                        Some(Err(error)) => view! { <p class="error-banner">{server_error_text(error)}</p> }.into_any(),
                        None => view! { <p class="result-line">"Loading invitations"</p> }.into_any(),
                    }}
                </div>
            </section>
        },
    )
}

#[island(lazy)]
pub fn OrganizationRolesPage() -> impl IntoView {
    let roles = browser_load(list_current_organization_roles);
    let upsert_action = ServerAction::<UpsertCurrentOrganizationRole>::new();
    let (role_id, set_role_id) = signal(String::new());
    let (name, set_name) = signal(String::new());
    let (permissions, set_permissions) = signal("organization.view,counter.view".to_owned());
    page_shell(
        "Roles",
        "Built-in roles are immutable; custom roles use the bounded tenant permission catalog.",
        view! {
            <section class="panel">
                <h2>"Role catalog"</h2>
                <div class="client-data-slot">
                    {move || match roles.get() {
                        Some(Ok(response)) => view! { <dl class="kv"><For each=move || response.roles.clone() key=|role| role.role_id.clone() children=move |role| view! {
                            <dt>{role.name}</dt><dd>{format!("{} permissions{}", role.permissions.len(), if role.built_in { " / built-in" } else { "" })}</dd>
                        } /></dl> }.into_any(),
                        Some(Err(error)) => view! { <p class="error-banner">{server_error_text(error)}</p> }.into_any(),
                        None => view! { <p class="result-line">"Loading roles"</p> }.into_any(),
                    }}
                </div>
            </section>
            <section class="panel">
                <h2>"Custom role"</h2>
                <label><span>"Role id"</span><input type="text" prop:value=move || role_id.get() on:input=move |event| set_role_id.set(event_target_value(&event)) /></label>
                <label><span>"Name"</span><input type="text" prop:value=move || name.get() on:input=move |event| set_name.set(event_target_value(&event)) /></label>
                <label><span>"Comma-separated permissions"</span><input type="text" prop:value=move || permissions.get() on:input=move |event| set_permissions.set(event_target_value(&event)) /></label>
                <button type="button" class="secondary-button" on:click=move |_| {
                    upsert_action.dispatch(UpsertCurrentOrganizationRole {
                        role_id: role_id.get_untracked(),
                        name: name.get_untracked(),
                        permissions: permissions.get_untracked().split(',').map(str::trim).filter(|value| !value.is_empty()).map(ToOwned::to_owned).collect(),
                    });
                }>"Save role"</button>
                <Show when=move || upsert_action.value().get().is_some()><p class="result-line">{move || action_result_text(upsert_action.value().get())}</p></Show>
            </section>
        },
    )
}

#[island(lazy)]
pub fn OrganizationPermissionsPage() -> impl IntoView {
    let roles = browser_load(list_current_organization_roles);
    page_shell(
        "Permissions",
        "Inspect effective permission assignments through organization roles.",
        view! {
            <section class="panel">
                <div class="client-data-slot">
                    {move || match roles.get() {
                        Some(Ok(response)) => view! { <div class="action-stack"><For each=move || response.roles.clone() key=|role| role.role_id.clone() children=move |role| view! {
                            <article class="compact-panel"><h3>{role.name}</h3><p class="result-line">{role.permissions.join(", ")}</p></article>
                        } /></div> }.into_any(),
                        Some(Err(error)) => view! { <p class="error-banner">{server_error_text(error)}</p> }.into_any(),
                        None => view! { <p class="result-line">"Loading permissions"</p> }.into_any(),
                    }}
                </div>
            </section>
        },
    )
}

#[island(lazy)]
pub fn OrganizationAuditPage() -> impl IntoView {
    let audit = browser_load(list_current_organization_audit);
    page_shell(
        "Audit activity",
        "Cursor-based audit reads share the same authorization path as the gRPC server stream.",
        view! {
            <section class="panel"><div class="client-data-slot">
                {move || match audit.get() {
                    Some(Ok(response)) => view! { <dl class="kv"><For each=move || response.events.clone() key=|event| event.sequence children=move |event| view! {
                        <dt>{event.action}</dt><dd>{format!("{}:{} / {}", event.target_type, event.target_id, event.outcome)}</dd>
                    } /></dl> }.into_any(),
                    Some(Err(error)) => view! { <p class="error-banner">{server_error_text(error)}</p> }.into_any(),
                    None => view! { <p class="result-line">"Loading audit activity"</p> }.into_any(),
                }}
            </div></section>
        },
    )
}
