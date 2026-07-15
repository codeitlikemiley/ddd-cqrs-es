//! Organization management UI.

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use crate::app::auth::SessionSummary;
use crate::app::helpers::{
    action_result_text, has_permission, org_monogram, org_tone_index, redirect_browser,
    server_error_text, short_id_label,
};
use crate::app::{
    CreateOrganization, InviteCurrentOrganizationMember, ListCurrentOrganizationAudit,
    ListCurrentOrganizationInvitations, ListCurrentOrganizationMembers,
    ListCurrentOrganizationRoles, ListOrganizations, SelectOrganization,
    UpsertCurrentOrganizationRole, browser_load, create_organization, get_current_session,
    invite_current_organization_member, list_current_organization_audit,
    list_current_organization_invitations, list_current_organization_members,
    list_current_organization_roles, list_organizations, select_organization,
    upsert_current_organization_role,
};
use crate::contracts::{
    AuditEventListResponse, InvitationListResponse, MembershipListResponse,
    OrganizationListResponse, OrganizationSummary, RoleListResponse, SessionView,
};
use crate::ui::{Panel, PrimaryButton, SectionLabel, page_shell};
use leptos::prelude::*;
use server_fn::ServerFnError;

#[island(lazy)]
pub fn OrganizationsPage() -> impl IntoView {
    page_shell(
        "Organizations",
        "Workspaces you belong to. Select one to scope members, roles, and audit.",
        view! { <OrganizationsHome /> },
    )
}

#[island]
pub fn OrganizationsHome() -> impl IntoView {
    let organizations = browser_load(list_organizations);
    let session = browser_load(get_current_session);
    let create_action = ServerAction::<CreateOrganization>::new();
    let create_pending = create_action.pending();
    let create_value = create_action.value();
    let select_action = ServerAction::<SelectOrganization>::new();
    let select_pending = select_action.pending();
    let select_value = select_action.value();
    let (name, set_name) = signal(String::new());
    let (slug, set_slug) = signal(String::new());
    let (slug_touched, set_slug_touched) = signal(false);
    let (create_open, set_create_open) = signal(false);

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

    Effect::new(move |_| {
        if matches!(create_value.get(), Some(Ok(_))) {
            set_name.set(String::new());
            set_slug.set(String::new());
            set_slug_touched.set(false);
            set_create_open.set(false);
            #[cfg(feature = "hydrate")]
            {
                // Refresh the list after a successful create.
                if let Some(window) = web_sys::window() {
                    let _ = window.location().reload();
                }
            }
        }
    });

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

    Effect::new(move |_| {
        let open = create_open.get();
        #[cfg(feature = "hydrate")]
        {
            if let Some(document) = web_sys::window().and_then(|window| window.document())
                && let Some(root) = document.document_element()
            {
                let _ = if open {
                    root.class_list().add_1("board-modal-open")
                } else {
                    root.class_list().remove_1("board-modal-open")
                };
            }
        }
        #[cfg(not(feature = "hydrate"))]
        {
            let _ = open;
        }
    });

    #[cfg(feature = "hydrate")]
    on_cleanup(|| {
        if let Some(document) = web_sys::window().and_then(|window| window.document())
            && let Some(root) = document.document_element()
        {
            let _ = root.class_list().remove_1("board-modal-open");
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

            <Show when=move || create_open.get()>
                <div
                    class="board-modal-backdrop orgs-create-backdrop"
                    role="presentation"
                    tabindex="-1"
                    on:click=move |_| {
                        if !create_pending.get_untracked() {
                            set_create_open.set(false);
                        }
                    }
                    on:keydown=move |event| {
                        if event.key() == "Escape" && !create_pending.get_untracked() {
                            set_create_open.set(false);
                        }
                    }
                    on:wheel=move |event| event.stop_propagation()
                >
                    <div
                        class="board-modal orgs-create-modal"
                        role="dialog"
                        aria-modal="true"
                        aria-labelledby="org-create-title"
                        aria-describedby="org-create-description"
                        on:click=move |event| event.stop_propagation()
                    >
                        <header class="board-modal-head">
                            <div>
                                <p class="dash-eyebrow">"New workspace"</p>
                                <h2 id="org-create-title">"Create organization"</h2>
                                <p id="org-create-description">
                                    "Choose a recognizable name and URL. You will become the owner."
                                </p>
                            </div>
                            <button
                                type="button"
                                class="board-modal-close"
                                disabled=move || create_pending.get()
                                on:click=move |_| set_create_open.set(false)
                            >
                                "Close"
                            </button>
                        </header>
                        <div class="board-modal-body orgs-create-body">
                            <form
                                class="orgs-create-form"
                                on:submit=move |event| {
                                    event.prevent_default();
                                    let value = name.get_untracked().trim().to_owned();
                                    let slug_value = slug.get_untracked().trim().to_owned();
                                    if value.is_empty() || slug_value.len() < 2 {
                                        return;
                                    }
                                    create_action.dispatch(CreateOrganization {
                                        name: value,
                                        slug: slug_value,
                                    });
                                }
                            >
                                <div class="orgs-create-fields">
                                    <label class="auth-field">
                                        <span>"Organization name"</span>
                                        <input
                                            class="auth-input"
                                            type="text"
                                            maxlength="120"
                                            autocomplete="organization"
                                            autofocus=true
                                            placeholder="Northwind Studio"
                                            prop:value=move || name.get()
                                            on:input=move |event| {
                                                let value = event_target_value(&event);
                                                set_name.set(value.clone());
                                                if !slug_touched.get_untracked() {
                                                    set_slug.set(derive_slug(&value));
                                                }
                                            }
                                        />
                                        <small>"Use the name teammates will recognize in the workspace switcher."</small>
                                    </label>
                                    <label class="auth-field">
                                        <span>"Workspace URL"</span>
                                        <div class="slug-input-group" role="group" aria-label="Workspace URL">
                                            <span class="slug-input-prefix" aria-hidden="true">"/org/"</span>
                                            <input
                                                class="auth-input slug-input-field mono-value"
                                                type="text"
                                                maxlength="48"
                                                autocomplete="off"
                                                placeholder="northwind"
                                                prop:value=move || slug.get()
                                                on:input=move |event| {
                                                    set_slug_touched.set(true);
                                                    set_slug.set(derive_slug(&event_target_value(&event)));
                                                }
                                            />
                                        </div>
                                        <small>"Lowercase letters, numbers, and hyphens only."</small>
                                    </label>
                                </div>
                                <Show when=move || {
                                    create_value.get().is_some_and(|result| result.is_err())
                                }>
                                    <p class="error-banner">{move || action_result_text(create_value.get())}</p>
                                </Show>
                                <div class="orgs-create-actions">
                                    <button
                                        type="button"
                                        class="secondary-button"
                                        disabled=move || create_pending.get()
                                        on:click=move |_| set_create_open.set(false)
                                    >
                                        "Cancel"
                                    </button>
                                    <button
                                        type="submit"
                                        class="primary-button"
                                        disabled=move || {
                                            create_pending.get()
                                                || name.get().trim().is_empty()
                                                || slug.get().trim().len() < 2
                                        }
                                    >
                                        {move || {
                                            if create_pending.get() {
                                                "Creating…"
                                            } else {
                                                "Create organization"
                                            }
                                        }}
                                    </button>
                                </div>
                            </form>
                        </div>
                    </div>
                </div>
            </Show>
        </div>
    }
}

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

#[component]
pub fn OrganizationLinks() -> impl IntoView {
    view! {
        <section class="panel panel-inline">
            <a class="text-link" href="/organizations">"Back to organizations"</a>
        </section>
    }
}
