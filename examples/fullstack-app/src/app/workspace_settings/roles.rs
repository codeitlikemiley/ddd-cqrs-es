//! Workspace settings — Roles & capabilities (list, create/edit custom, delete).

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use super::shared::{settings_page_stub, slug_from_settings_pathname};
use crate::app::helpers::{current_browser_pathname, server_error_text};
use crate::app::{
    DeleteWorkspaceRole, UpsertWorkspaceRole, browser_load, get_workspace_settings_context,
    list_workspace_members, list_workspace_permissions, list_workspace_roles,
};
use crate::contracts::{PermissionOption, RoleSummary};
use leptos::prelude::*;
#[cfg(feature = "hydrate")]
use leptos::task::spawn_local;
use std::collections::{BTreeMap, BTreeSet};

#[component]
pub fn WorkspaceSettingsRolesPage() -> impl IntoView {
    settings_page_stub(
        "Roles",
        "Built-in and custom roles for this workspace.",
        "Requires role.view. Create, edit, and delete need role.manage and step-up (AAL2).",
        view! { <WorkspaceSettingsRolesBody /> },
    )
}

/// Island: role catalog, custom role editor, delete with confirm.
#[island]
pub fn WorkspaceSettingsRolesBody() -> impl IntoView {
    let slug = Memo::new(move |_| slug_from_settings_pathname(&current_browser_pathname()));

    let roles = browser_load({
        move || {
            let slug = slug_from_settings_pathname(&current_browser_pathname());
            list_workspace_roles(slug)
        }
    });
    let permissions = browser_load({
        move || {
            let slug = slug_from_settings_pathname(&current_browser_pathname());
            list_workspace_permissions(slug)
        }
    });
    let members = browser_load({
        move || {
            let slug = slug_from_settings_pathname(&current_browser_pathname());
            list_workspace_members(slug)
        }
    });
    let context = browser_load({
        move || {
            let slug = slug_from_settings_pathname(&current_browser_pathname());
            get_workspace_settings_context(slug)
        }
    });

    let upsert = ServerAction::<UpsertWorkspaceRole>::new();
    let delete = ServerAction::<DeleteWorkspaceRole>::new();
    let upsert_pending = upsert.pending();
    let delete_pending = delete.pending();
    let upsert_value = upsert.value();
    let delete_value = delete.value();

    let (rows, set_rows) = signal(Vec::<RoleSummary>::new());
    let (catalog, set_catalog) = signal(Vec::<PermissionOption>::new());
    let (member_counts, set_member_counts) = signal(BTreeMap::<String, u32>::new());
    let (requires_step_up, set_requires_step_up) = signal(false);
    let (list_ready, set_list_ready) = signal(false);
    let (action_error, set_action_error) = signal(None::<String>);
    let (action_ok, set_action_ok) = signal(None::<String>);

    // Editor state
    let (editor_open, set_editor_open) = signal(false);
    let (editing_role_id, set_editing_role_id) = signal(None::<String>);
    let (role_id_input, set_role_id_input) = signal(String::new());
    let (role_name, set_role_name) = signal(String::new());
    let (selected_perms, set_selected_perms) = signal(BTreeSet::<String>::new());
    let (role_id_locked, set_role_id_locked) = signal(false);

    let (delete_target, set_delete_target) = signal(None::<(String, String)>);
    let (busy_role_id, set_busy_role_id) = signal(None::<String>);

    Effect::new(move |_| {
        if let Some(Ok(list)) = roles.get() {
            set_rows.set(list.roles);
            set_list_ready.set(true);
        }
    });

    Effect::new(move |_| {
        if let Some(Ok(catalog_response)) = permissions.get() {
            if catalog_response.options.is_empty() {
                set_catalog.set(
                    catalog_response
                        .permissions
                        .into_iter()
                        .map(|id| PermissionOption {
                            label: humanize_permission_id(&id),
                            group: "Catalog".to_owned(),
                            id,
                        })
                        .collect(),
                );
            } else {
                set_catalog.set(catalog_response.options);
            }
        }
    });

    Effect::new(move |_| {
        if let Some(Ok(list)) = members.get() {
            let mut counts = BTreeMap::<String, u32>::new();
            for membership in list.memberships {
                if membership.status == "active" {
                    *counts.entry(membership.role_id).or_insert(0) += 1;
                }
            }
            set_member_counts.set(counts);
        }
    });

    Effect::new(move |_| {
        if let Some(Ok(ctx)) = context.get() {
            set_requires_step_up.set(ctx.requires_step_up);
        }
    });

    Effect::new(move |_| match upsert_value.get() {
        Some(Ok(saved)) => {
            set_rows.update(|list| {
                if let Some(existing) = list.iter_mut().find(|row| row.role_id == saved.role_id) {
                    *existing = saved;
                } else {
                    list.push(saved);
                    list.sort_by(|a, b| {
                        b.built_in
                            .cmp(&a.built_in)
                            .then_with(|| a.name.cmp(&b.name))
                            .then_with(|| a.role_id.cmp(&b.role_id))
                    });
                }
            });
            close_editor(
                set_editor_open,
                set_editing_role_id,
                set_role_id_input,
                set_role_name,
                set_selected_perms,
                set_role_id_locked,
            );
            set_busy_role_id.set(None);
            set_action_error.set(None);
            set_action_ok.set(Some("Role saved.".to_owned()));
        }
        Some(Err(error)) => {
            set_busy_role_id.set(None);
            set_action_ok.set(None);
            set_action_error.set(Some(server_error_text(error)));
        }
        None => {}
    });

    Effect::new(move |_| match delete_value.get() {
        Some(Ok(_)) => {
            if let Some((role_id, _)) = delete_target.get_untracked() {
                set_rows.update(|list| list.retain(|row| row.role_id != role_id));
            }
            set_delete_target.set(None);
            set_busy_role_id.set(None);
            set_action_error.set(None);
            set_action_ok.set(Some("Custom role deleted.".to_owned()));
        }
        Some(Err(error)) => {
            set_busy_role_id.set(None);
            set_action_ok.set(None);
            set_action_error.set(Some(server_error_text(error)));
        }
        None => {}
    });

    let load_error = Memo::new(move |_| {
        roles.get().and_then(|result| match result {
            Ok(_) => None,
            Err(error) => Some(server_error_text(error)),
        })
    });

    let any_busy = Memo::new(move |_| upsert_pending.get() || delete_pending.get());

    let can_save = Memo::new(move |_| {
        !any_busy.get()
            && !role_name.get().trim().is_empty()
            && !role_id_input.get().trim().is_empty()
            && !selected_perms.get().is_empty()
            && !slug.get().is_empty()
    });

    view! {
        <Show when=move || roles.get().is_none()>
            <p class="result-line" aria-busy="true">"Loading roles…"</p>
        </Show>

        <Show when=move || load_error.get().is_some()>
            <p class="error-banner">{move || load_error.get().unwrap_or_default()}</p>
        </Show>

        <Show when=move || requires_step_up.get()>
            <p class="workspace-settings-step-up" role="status">
                "Creating, editing, and deleting roles requires a step-up session (AAL2). "
                <a href="/account/mfa">"Complete MFA"</a>
                " if your session is not elevated."
            </p>
        </Show>

        <Show when=move || action_error.get().is_some()>
            <p class="error-banner">{move || action_error.get().unwrap_or_default()}</p>
        </Show>

        <Show when=move || action_ok.get().is_some()>
            <p class="result-line" role="status">{move || action_ok.get().unwrap_or_default()}</p>
        </Show>

        <div class="workspace-settings-roles-toolbar">
            <button
                type="button"
                class="primary-button"
                disabled=move || any_busy.get() || editor_open.get()
                on:click=move |_| {
                    set_action_error.set(None);
                    set_action_ok.set(None);
                    open_create_editor(
                        set_editor_open,
                        set_editing_role_id,
                        set_role_id_input,
                        set_role_name,
                        set_selected_perms,
                        set_role_id_locked,
                    );
                }
            >
                "Create custom role"
            </button>
        </div>

        <Show when=move || editor_open.get()>
            <form
                class="workspace-settings-role-form"
                on:submit=move |event| {
                    event.prevent_default();
                    let slug_value = slug.get_untracked();
                    let mut id = role_id_input.get_untracked().trim().to_owned();
                    let name = role_name.get_untracked().trim().to_owned();
                    let perms: Vec<String> = selected_perms.get_untracked().into_iter().collect();
                    if slug_value.is_empty() || name.is_empty() {
                        set_action_error.set(Some("role name is required".to_owned()));
                        return;
                    }
                    if id.is_empty() {
                        id = slugify_role_id(&name);
                        set_role_id_input.set(id.clone());
                    }
                    if !is_valid_role_id(&id) {
                        set_action_error.set(Some(
                            "role id must be 1–128 characters: letters, numbers, ., _, -"
                                .to_owned(),
                        ));
                        return;
                    }
                    if matches!(id.as_str(), "owner" | "admin" | "member" | "viewer") {
                        set_action_error.set(Some(
                            "built-in role ids cannot be used for custom roles".to_owned(),
                        ));
                        return;
                    }
                    if perms.is_empty() {
                        set_action_error.set(Some(
                            "select at least one permission".to_owned(),
                        ));
                        return;
                    }
                    set_action_error.set(None);
                    set_action_ok.set(None);
                    set_busy_role_id.set(Some(id.clone()));
                    upsert.dispatch(UpsertWorkspaceRole {
                        slug: slug_value,
                        role_id: id,
                        name,
                        permissions: perms,
                    });
                }
            >
                <div class="workspace-settings-role-form-head">
                    <h2>
                        {move || {
                            if editing_role_id.get().is_some() {
                                "Edit custom role"
                            } else {
                                "Create custom role"
                            }
                        }}
                    </h2>
                    <p class="board-muted">
                        "Permissions expand with required dependencies on save. Built-in roles stay immutable."
                    </p>
                </div>
                <label class="auth-field">
                    <span>"Name"</span>
                    <input
                        class="auth-input"
                        type="text"
                        maxlength="80"
                        prop:value=move || role_name.get()
                        on:input=move |event| {
                            let value = event_target_value(&event);
                            set_role_name.set(value.clone());
                            if !role_id_locked.get_untracked()
                                && editing_role_id.get_untracked().is_none()
                            {
                                set_role_id_input.set(slugify_role_id(&value));
                            }
                            set_action_ok.set(None);
                        }
                        disabled=move || any_busy.get()
                    />
                </label>
                <label class="auth-field">
                    <span>"Role id"</span>
                    <input
                        class="auth-input"
                        type="text"
                        maxlength="128"
                        prop:value=move || role_id_input.get()
                        on:input=move |event| {
                            set_role_id_locked.set(true);
                            set_role_id_input.set(event_target_value(&event));
                            set_action_ok.set(None);
                        }
                        disabled=move || any_busy.get() || editing_role_id.get().is_some()
                    />
                </label>
                <fieldset class="workspace-settings-permission-fieldset">
                    <legend>"Permissions"</legend>
                    {move || {
                        let options = catalog.get();
                        if options.is_empty() {
                            return view! {
                                <p class="board-muted">
                                    "Permission catalog is loading or unavailable."
                                </p>
                            }
                            .into_any();
                        }
                        let mut groups: BTreeMap<String, Vec<PermissionOption>> = BTreeMap::new();
                        for option in options {
                            groups.entry(option.group.clone()).or_default().push(option);
                        }
                        groups
                            .into_iter()
                            .map(|(group, items)| {
                                view! {
                                    <div class="workspace-settings-permission-group">
                                        <h3>{group}</h3>
                                        <div class="workspace-settings-permission-grid">
                                            {items
                                                .into_iter()
                                                .map(|option| {
                                                    let id = option.id.clone();
                                                    let id_for_check = id.clone();
                                                    let id_for_toggle = id.clone();
                                                    let label = option.label.clone();
                                                    view! {
                                                        <label class="workspace-settings-permission-option">
                                                            <input
                                                                type="checkbox"
                                                                prop:checked=move || {
                                                                    selected_perms.get().contains(&id_for_check)
                                                                }
                                                                disabled=move || any_busy.get()
                                                                on:change=move |_| {
                                                                    set_selected_perms.update(|set| {
                                                                        if set.contains(&id_for_toggle) {
                                                                            set.remove(&id_for_toggle);
                                                                        } else {
                                                                            set.insert(id_for_toggle.clone());
                                                                        }
                                                                    });
                                                                    set_action_ok.set(None);
                                                                }
                                                            />
                                                            <span>
                                                                <strong>{label}</strong>
                                                                <small class="board-muted">{id}</small>
                                                            </span>
                                                        </label>
                                                    }
                                                })
                                                .collect_view()}
                                        </div>
                                    </div>
                                }
                            })
                            .collect_view()
                            .into_any()
                    }}
                </fieldset>
                <div class="workspace-settings-role-form-actions">
                    <button
                        type="button"
                        class="secondary-button"
                        disabled=move || any_busy.get()
                        on:click=move |_| {
                            close_editor(
                                set_editor_open,
                                set_editing_role_id,
                                set_role_id_input,
                                set_role_name,
                                set_selected_perms,
                                set_role_id_locked,
                            );
                        }
                    >
                        "Cancel"
                    </button>
                    <button
                        type="submit"
                        class="primary-button"
                        disabled=move || !can_save.get()
                    >
                        {move || {
                            if upsert_pending.get() {
                                "Saving…"
                            } else if editing_role_id.get().is_some() {
                                "Save changes"
                            } else {
                                "Create role"
                            }
                        }}
                    </button>
                </div>
            </form>
        </Show>

        {move || {
            if !list_ready.get() {
                return view! { <></> }.into_any();
            }
            let list = rows.get();
            if list.is_empty() {
                return view! {
                    <div class="workspace-settings-empty" role="status">
                        <p>"No roles found."</p>
                    </div>
                }
                .into_any();
            }
            let counts = member_counts.get();
            let busy = busy_role_id.get();
            let actions_locked = any_busy.get();
            view! {
                <div class="table-wrap workspace-settings-table-wrap">
                    <table class="data-table workspace-settings-roles-table">
                        <thead>
                            <tr>
                                <th scope="col">"Role"</th>
                                <th scope="col">"Type"</th>
                                <th scope="col">"Permissions"</th>
                                <th scope="col">"Members"</th>
                                <th scope="col">"Actions"</th>
                            </tr>
                        </thead>
                        <tbody>
                            {list
                                .into_iter()
                                .map(|role| {
                                    role_row(
                                        role,
                                        counts.clone(),
                                        busy.clone(),
                                        actions_locked,
                                        slug,
                                        set_editor_open,
                                        set_editing_role_id,
                                        set_role_id_input,
                                        set_role_name,
                                        set_selected_perms,
                                        set_role_id_locked,
                                        set_delete_target,
                                        set_action_error,
                                        set_action_ok,
                                    )
                                })
                                .collect_view()}
                        </tbody>
                    </table>
                </div>
            }
            .into_any()
        }}

        <Show when=move || delete_target.get().is_some()>
            <div
                class="board-modal-backdrop"
                role="presentation"
                on:click=move |_| {
                    if !delete_pending.get() {
                        set_delete_target.set(None);
                    }
                }
            >
                <div
                    class="board-modal vault-modal-confirm"
                    role="dialog"
                    aria-modal="true"
                    aria-labelledby="workspace-delete-role-title"
                    on:click=move |e| e.stop_propagation()
                >
                    <header class="board-modal-head">
                        <div>
                            <h2 id="workspace-delete-role-title">"Delete custom role?"</h2>
                            <p>
                                "Delete "
                                <strong>
                                    {move || {
                                        delete_target
                                            .get()
                                            .map(|(_, name)| name)
                                            .unwrap_or_default()
                                    }}
                                </strong>
                                ". Active members and pending invitations must be reassigned first."
                            </p>
                        </div>
                        <button
                            type="button"
                            class="board-modal-close"
                            disabled=move || delete_pending.get()
                            on:click=move |_| set_delete_target.set(None)
                        >
                            "Close"
                        </button>
                    </header>
                    <div class="board-modal-body">
                        <div class="workspace-settings-modal-actions">
                            <button
                                type="button"
                                class="secondary-button"
                                disabled=move || delete_pending.get()
                                on:click=move |_| set_delete_target.set(None)
                            >
                                "Cancel"
                            </button>
                            <button
                                type="button"
                                class="primary-button workspace-settings-danger-button"
                                disabled=move || delete_pending.get()
                                on:click=move |_| {
                                    let Some((role_id, _)) = delete_target.get_untracked() else {
                                        return;
                                    };
                                    let slug_value = slug.get_untracked();
                                    if slug_value.is_empty() || role_id.is_empty() {
                                        set_action_error.set(Some(
                                            "missing workspace or role".to_owned(),
                                        ));
                                        return;
                                    }
                                    set_action_error.set(None);
                                    set_action_ok.set(None);
                                    set_busy_role_id.set(Some(role_id.clone()));
                                    delete.dispatch(DeleteWorkspaceRole {
                                        slug: slug_value,
                                        role_id,
                                    });
                                }
                            >
                                {move || {
                                    if delete_pending.get() {
                                        "Deleting…"
                                    } else {
                                        "Delete role"
                                    }
                                }}
                            </button>
                        </div>
                    </div>
                </div>
            </div>
        </Show>
    }
}

fn role_row(
    role: RoleSummary,
    member_counts: BTreeMap<String, u32>,
    busy_role_id: Option<String>,
    actions_locked: bool,
    _slug: Memo<String>,
    set_editor_open: WriteSignal<bool>,
    set_editing_role_id: WriteSignal<Option<String>>,
    set_role_id_input: WriteSignal<String>,
    set_role_name: WriteSignal<String>,
    set_selected_perms: WriteSignal<BTreeSet<String>>,
    set_role_id_locked: WriteSignal<bool>,
    set_delete_target: WriteSignal<Option<(String, String)>>,
    set_action_error: WriteSignal<Option<String>>,
    set_action_ok: WriteSignal<Option<String>>,
) -> impl IntoView {
    let role_id = role.role_id.clone();
    let role_id_edit = role_id.clone();
    let role_id_delete = role_id.clone();
    let role_id_busy = role_id.clone();
    let name = if role.name.trim().is_empty() {
        role_id.clone()
    } else {
        role.name.clone()
    };
    let name_for_edit = name.clone();
    let name_for_delete = name.clone();
    let built_in = role.built_in;
    let permission_count = role.permissions.len();
    let members = member_counts.get(&role_id).copied().unwrap_or(0);
    let members_label = if member_counts.is_empty() {
        "—".to_owned()
    } else {
        members.to_string()
    };
    let row_busy = busy_role_id.as_deref() == Some(role_id.as_str());
    let disabled = actions_locked || row_busy;
    let perms_for_edit = role.permissions.clone();

    view! {
        <tr class="workspace-settings-role-row">
            <td data-label="Role">
                <div class="workspace-settings-role-name">
                    <strong>{name}</strong>
                    <small class="board-muted">{role_id.clone()}</small>
                </div>
            </td>
            <td data-label="Type">
                <span class=if built_in {
                    "workspace-settings-role-badge is-built-in"
                } else {
                    "workspace-settings-role-badge is-custom"
                }>
                    {if built_in { "Built-in" } else { "Custom" }}
                </span>
            </td>
            <td data-label="Permissions">{permission_count}</td>
            <td data-label="Members">{members_label}</td>
            <td class="workspace-settings-member-actions" data-label="Actions">
                {if built_in {
                    view! {
                        <span class="board-muted" title="Built-in roles are immutable">
                            "Immutable"
                        </span>
                    }
                    .into_any()
                } else {
                    view! {
                        <div class="workspace-settings-role-actions">
                            <button
                                type="button"
                                class="secondary-button"
                                disabled=disabled
                                on:click=move |_| {
                                    set_action_error.set(None);
                                    set_action_ok.set(None);
                                    set_editor_open.set(true);
                                    set_editing_role_id.set(Some(role_id_edit.clone()));
                                    set_role_id_input.set(role_id_edit.clone());
                                    set_role_name.set(name_for_edit.clone());
                                    set_selected_perms.set(perms_for_edit.iter().cloned().collect());
                                    set_role_id_locked.set(true);
                                }
                            >
                                "Edit"
                            </button>
                            <button
                                type="button"
                                class="secondary-button workspace-settings-remove-button"
                                disabled=disabled
                                on:click=move |_| {
                                    set_action_error.set(None);
                                    set_action_ok.set(None);
                                    set_delete_target.set(Some((
                                        role_id_delete.clone(),
                                        name_for_delete.clone(),
                                    )));
                                }
                            >
                                {if busy_role_id.as_deref() == Some(role_id_busy.as_str())
                                    && actions_locked
                                {
                                    "Working…"
                                } else {
                                    "Delete"
                                }}
                            </button>
                        </div>
                    }
                    .into_any()
                }}
            </td>
        </tr>
    }
}

fn open_create_editor(
    set_editor_open: WriteSignal<bool>,
    set_editing_role_id: WriteSignal<Option<String>>,
    set_role_id_input: WriteSignal<String>,
    set_role_name: WriteSignal<String>,
    set_selected_perms: WriteSignal<BTreeSet<String>>,
    set_role_id_locked: WriteSignal<bool>,
) {
    set_editor_open.set(true);
    set_editing_role_id.set(None);
    set_role_id_input.set(String::new());
    set_role_name.set(String::new());
    set_selected_perms.set(BTreeSet::new());
    set_role_id_locked.set(false);
}

fn close_editor(
    set_editor_open: WriteSignal<bool>,
    set_editing_role_id: WriteSignal<Option<String>>,
    set_role_id_input: WriteSignal<String>,
    set_role_name: WriteSignal<String>,
    set_selected_perms: WriteSignal<BTreeSet<String>>,
    set_role_id_locked: WriteSignal<bool>,
) {
    set_editor_open.set(false);
    set_editing_role_id.set(None);
    set_role_id_input.set(String::new());
    set_role_name.set(String::new());
    set_selected_perms.set(BTreeSet::new());
    set_role_id_locked.set(false);
}

fn slugify_role_id(name: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in name.trim().chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
            last_dash = false;
        } else if matches!(lower, '.' | '_' | '-') {
            if !out.is_empty() && !last_dash {
                out.push(lower);
                last_dash = lower == '-';
            }
        } else if !out.is_empty() && !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.len() > 128 {
        out.truncate(128);
    }
    out
}

fn is_valid_role_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn humanize_permission_id(id: &str) -> String {
    id.split('.')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => {
                    format!("{}{}", first.to_ascii_uppercase(), chars.as_str())
                }
                None => String::new(),
            }
        })
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
