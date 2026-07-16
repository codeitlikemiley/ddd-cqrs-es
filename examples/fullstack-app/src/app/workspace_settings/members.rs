//! Workspace settings — Members (table, role assign, remove confirm).

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use super::shared::{
    display_role_name, format_settings_timestamp_ms, role_label_from_options, settings_page_stub,
    slug_from_settings_pathname,
};
use crate::app::helpers::{current_browser_pathname, server_error_text};
use crate::app::{
    AssignWorkspaceMemberRole, RemoveWorkspaceMember, assign_workspace_member_role, browser_load,
    get_current_session, get_workspace_settings_context, list_workspace_members,
    remove_workspace_member,
};
use crate::contracts::{MembershipSummary, WorkspaceRoleOption};
use leptos::prelude::*;
#[cfg(feature = "hydrate")]
use leptos::task::spawn_local;

#[component]
pub fn WorkspaceSettingsMembersPage() -> impl IntoView {
    settings_page_stub(
        "Members",
        "People who belong to this workspace and their roles.",
        "Requires member.view. Role changes and removals need step-up (AAL2).",
        view! { <WorkspaceSettingsMembersBody /> },
    )
}

/// Island: list members; assign roles from server options; remove with confirm.
#[island]
pub fn WorkspaceSettingsMembersBody() -> impl IntoView {
    let slug = Memo::new(move |_| slug_from_settings_pathname(&current_browser_pathname()));

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
    let session = browser_load(get_current_session);

    let assign = ServerAction::<AssignWorkspaceMemberRole>::new();
    let remove = ServerAction::<RemoveWorkspaceMember>::new();
    let assign_pending = assign.pending();
    let remove_pending = remove.pending();
    let assign_value = assign.value();
    let remove_value = remove.value();

    let (rows, set_rows) = signal(Vec::<MembershipSummary>::new());
    let (role_options, set_role_options) = signal(Vec::<WorkspaceRoleOption>::new());
    let (current_user_id, set_current_user_id) = signal(None::<String>);
    let (requires_step_up, set_requires_step_up) = signal(false);
    let (busy_user_id, set_busy_user_id) = signal(None::<String>);
    let (remove_target, set_remove_target) = signal(None::<(String, String)>);
    let (action_error, set_action_error) = signal(None::<String>);
    let (action_ok, set_action_ok) = signal(None::<String>);
    let (list_ready, set_list_ready) = signal(false);

    Effect::new(move |_| {
        if let Some(Ok(list)) = members.get() {
            set_rows.set(list.memberships);
            set_list_ready.set(true);
        }
    });

    Effect::new(move |_| {
        if let Some(Ok(ctx)) = context.get() {
            set_role_options.set(ctx.role_options);
            set_requires_step_up.set(ctx.requires_step_up);
        }
    });

    Effect::new(move |_| {
        if let Some(Ok(sess)) = session.get() {
            set_current_user_id.set(sess.user_id.filter(|id| !id.trim().is_empty()));
        }
    });

    Effect::new(move |_| match assign_value.get() {
        Some(Ok(updated)) => {
            set_rows.update(|list| {
                if let Some(row) = list.iter_mut().find(|m| m.user_id == updated.user_id) {
                    *row = updated;
                }
            });
            set_busy_user_id.set(None);
            set_action_error.set(None);
            set_action_ok.set(Some("Member role updated.".to_owned()));
        }
        Some(Err(error)) => {
            set_busy_user_id.set(None);
            set_action_ok.set(None);
            set_action_error.set(Some(server_error_text(error)));
            reload_members_list(slug.get_untracked(), set_rows, set_action_error);
        }
        None => {}
    });

    Effect::new(move |_| match remove_value.get() {
        Some(Ok(_)) => {
            if let Some((user_id, _)) = remove_target.get_untracked() {
                set_rows.update(|list| list.retain(|m| m.user_id != user_id));
            }
            set_remove_target.set(None);
            set_busy_user_id.set(None);
            set_action_error.set(None);
            set_action_ok.set(Some("Member removed.".to_owned()));
        }
        Some(Err(error)) => {
            set_busy_user_id.set(None);
            set_action_ok.set(None);
            set_action_error.set(Some(server_error_text(error)));
        }
        None => {}
    });

    let load_error = Memo::new(move |_| {
        members.get().and_then(|result| match result {
            Ok(_) => None,
            Err(error) => Some(server_error_text(error)),
        })
    });

    let any_busy = Memo::new(move |_| assign_pending.get() || remove_pending.get());

    view! {
        <Show when=move || members.get().is_none()>
            <p class="result-line" aria-busy="true">"Loading members…"</p>
        </Show>

        <Show when=move || load_error.get().is_some()>
            <p class="error-banner">{move || load_error.get().unwrap_or_default()}</p>
        </Show>

        <Show when=move || requires_step_up.get()>
            <p class="workspace-settings-step-up" role="status">
                "Role changes and removals require a step-up session (AAL2). "
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

        {move || {
            if !list_ready.get() {
                return view! { <></> }.into_any();
            }
            let list = rows.get();
            if list.is_empty() {
                return view! {
                    <div class="workspace-settings-empty" role="status">
                        <p>"No members found."</p>
                    </div>
                }
                .into_any();
            }

            let options = role_options.get();
            let me = current_user_id.get();
            let busy = busy_user_id.get();
            let actions_locked = any_busy.get();

            view! {
                <div class="table-wrap workspace-settings-table-wrap">
                    <table class="data-table workspace-settings-members-table">
                        <thead>
                            <tr>
                                <th scope="col">"Email"</th>
                                <th scope="col">"Role"</th>
                                <th scope="col">"Status"</th>
                                <th scope="col">"Joined"</th>
                                <th scope="col">"Actions"</th>
                            </tr>
                        </thead>
                        <tbody>
                            {list
                                .into_iter()
                                .map(|member| {
                                    member_row(
                                        member,
                                        options.clone(),
                                        me.clone(),
                                        busy.clone(),
                                        actions_locked,
                                        slug,
                                        assign,
                                        set_busy_user_id,
                                        set_remove_target,
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

        <Show when=move || remove_target.get().is_some()>
            <div
                class="board-modal-backdrop"
                role="presentation"
                on:click=move |_| {
                    if !remove_pending.get() {
                        set_remove_target.set(None);
                    }
                }
            >
                <div
                    class="board-modal vault-modal-confirm"
                    role="dialog"
                    aria-modal="true"
                    aria-labelledby="workspace-remove-member-title"
                    on:click=move |e| e.stop_propagation()
                >
                    <header class="board-modal-head">
                        <div>
                            <h2 id="workspace-remove-member-title">"Remove member?"</h2>
                            <p>
                                "Remove "
                                <strong>
                                    {move || {
                                        remove_target
                                            .get()
                                            .map(|(_, email)| email)
                                            .unwrap_or_default()
                                    }}
                                </strong>
                                " from this workspace. They lose access immediately."
                            </p>
                        </div>
                        <button
                            type="button"
                            class="board-modal-close"
                            disabled=move || remove_pending.get()
                            on:click=move |_| set_remove_target.set(None)
                        >
                            "Close"
                        </button>
                    </header>
                    <div class="board-modal-body">
                        <div class="workspace-settings-modal-actions">
                            <button
                                type="button"
                                class="secondary-button"
                                disabled=move || remove_pending.get()
                                on:click=move |_| set_remove_target.set(None)
                            >
                                "Cancel"
                            </button>
                            <button
                                type="button"
                                class="primary-button workspace-settings-danger-button"
                                disabled=move || remove_pending.get()
                                on:click=move |_| {
                                    let Some((user_id, _)) = remove_target.get_untracked() else {
                                        return;
                                    };
                                    let slug_value = slug.get_untracked();
                                    if slug_value.is_empty() || user_id.is_empty() {
                                        set_action_error.set(Some(
                                            "missing workspace or member".to_owned(),
                                        ));
                                        return;
                                    }
                                    set_action_error.set(None);
                                    set_action_ok.set(None);
                                    set_busy_user_id.set(Some(user_id.clone()));
                                    remove.dispatch(RemoveWorkspaceMember {
                                        slug: slug_value,
                                        user_id,
                                    });
                                }
                            >
                                {move || {
                                    if remove_pending.get() {
                                        "Removing…"
                                    } else {
                                        "Remove member"
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

fn member_row(
    member: MembershipSummary,
    role_options: Vec<WorkspaceRoleOption>,
    current_user_id: Option<String>,
    busy_user_id: Option<String>,
    actions_locked: bool,
    slug: Memo<String>,
    assign: ServerAction<AssignWorkspaceMemberRole>,
    set_busy_user_id: WriteSignal<Option<String>>,
    set_remove_target: WriteSignal<Option<(String, String)>>,
    set_action_error: WriteSignal<Option<String>>,
    set_action_ok: WriteSignal<Option<String>>,
) -> impl IntoView {
    let user_id = member.user_id.clone();
    let user_id_for_role = user_id.clone();
    let user_id_for_remove = user_id.clone();
    let user_id_busy = user_id.clone();
    let email = if member.primary_email.trim().is_empty() {
        "—".to_owned()
    } else {
        member.primary_email.clone()
    };
    let email_for_remove = if member.primary_email.trim().is_empty() {
        user_id.clone()
    } else {
        member.primary_email.clone()
    };
    let status = member.status.clone();
    let joined = format_settings_timestamp_ms(member.joined_at_ms);
    let role_id = member.role_id.clone();
    let is_you = current_user_id
        .as_deref()
        .is_some_and(|id| id == user_id.as_str());
    let is_owner = role_id == "owner";
    let row_busy = busy_user_id.as_deref() == Some(user_id.as_str());
    let can_change_role = !is_owner && !role_options.is_empty();
    let role_label = role_label_from_options(&role_id, &role_options);
    let select_options = role_options.clone();
    let current_role = role_id.clone();

    view! {
        <tr class=if is_you {
            "workspace-settings-member-row is-you"
        } else {
            "workspace-settings-member-row"
        }>
            <td>
                <span class="workspace-settings-member-email">{email}</span>
                {if is_you {
                    view! { <span class="workspace-settings-you-badge">"You"</span> }.into_any()
                } else {
                    view! { <></> }.into_any()
                }}
            </td>
            <td>
                {if can_change_role {
                    let select_disabled = actions_locked || row_busy;
                    view! {
                        <label class="workspace-settings-role-select-label">
                            <span class="sr-only">"Role"</span>
                            <select
                                class="workspace-settings-role-select"
                                prop:value=current_role.clone()
                                disabled=select_disabled
                                on:change=move |event| {
                                    let next = event_target_value(&event);
                                    if next.is_empty() || next == current_role {
                                        return;
                                    }
                                    if !select_options.iter().any(|opt| opt.role_id == next) {
                                        set_action_error.set(Some(
                                            "role is not available for assignment".to_owned(),
                                        ));
                                        return;
                                    }
                                    let slug_value = slug.get_untracked();
                                    if slug_value.is_empty() {
                                        set_action_error.set(Some(
                                            "missing workspace".to_owned(),
                                        ));
                                        return;
                                    }
                                    set_action_error.set(None);
                                    set_action_ok.set(None);
                                    set_busy_user_id.set(Some(user_id_for_role.clone()));
                                    assign.dispatch(AssignWorkspaceMemberRole {
                                        slug: slug_value,
                                        user_id: user_id_for_role.clone(),
                                        role_id: next,
                                    });
                                }
                            >
                                {select_options
                                    .iter()
                                    .map(|opt| {
                                        let id = opt.role_id.clone();
                                        let label = if opt.name.trim().is_empty() {
                                            display_role_name(&opt.role_id)
                                        } else {
                                            opt.name.clone()
                                        };
                                        let selected = id == role_id;
                                        view! {
                                            <option value=id selected=selected>
                                                {label}
                                            </option>
                                        }
                                    })
                                    .collect_view()}
                            </select>
                        </label>
                    }
                    .into_any()
                } else {
                    view! { <span>{role_label}</span> }.into_any()
                }}
            </td>
            <td>{status}</td>
            <td>{joined}</td>
            <td class="workspace-settings-member-actions">
                {if is_you {
                    view! {
                        <span
                            class="board-muted workspace-settings-self-remove-hint"
                            title="Leaving the workspace is not available here yet"
                        >
                            "Use Danger zone to leave (coming soon)"
                        </span>
                    }
                    .into_any()
                } else {
                    let disabled = actions_locked || row_busy;
                    view! {
                        <button
                            type="button"
                            class="secondary-button workspace-settings-remove-button"
                            disabled=disabled
                            on:click=move |_| {
                                set_action_error.set(None);
                                set_action_ok.set(None);
                                set_remove_target.set(Some((
                                    user_id_for_remove.clone(),
                                    email_for_remove.clone(),
                                )));
                            }
                        >
                            {if busy_user_id.as_deref() == Some(user_id_busy.as_str())
                                && actions_locked
                            {
                                "Working…"
                            } else {
                                "Remove"
                            }}
                        </button>
                    }
                    .into_any()
                }}
            </td>
        </tr>
    }
}

fn reload_members_list(
    slug: String,
    set_rows: WriteSignal<Vec<MembershipSummary>>,
    set_action_error: WriteSignal<Option<String>>,
) {
    if slug.is_empty() {
        return;
    }
    #[cfg(feature = "hydrate")]
    {
        spawn_local(async move {
            match list_workspace_members(slug).await {
                Ok(list) => set_rows.set(list.memberships),
                Err(error) => set_action_error.set(Some(server_error_text(error))),
            }
        });
    }
    #[cfg(not(feature = "hydrate"))]
    {
        let _ = (set_rows, set_action_error);
    }
}
