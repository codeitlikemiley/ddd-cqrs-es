//! Workspace settings — Members (table, role assign, remove, transfer ownership).

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use super::shared::{
    display_role_name, format_settings_timestamp_ms, role_label_from_options, settings_page_stub,
    slug_from_settings_pathname,
};
use crate::app::helpers::{current_browser_pathname, server_error_text};
use crate::app::{
    AssignWorkspaceMemberRole, RemoveWorkspaceMember, TransferWorkspaceOwnership,
    assign_workspace_member_role, browser_load, get_current_session, get_workspace_settings_context,
    list_workspace_members, remove_workspace_member, transfer_workspace_ownership,
};
use crate::contracts::{MembershipSummary, WorkspaceRoleOption};
use leptos::prelude::*;
#[cfg(feature = "hydrate")]
use leptos::task::spawn_local;
use crate::ui::classes::{
    AUTH_TEXT_LINK, BANNER_ERROR, BANNER_SUCCESS, BTN_AUTH_SUBMIT, BTN_PRIMARY, BTN_SECONDARY,
    BUTTON_ROW, FIELD, FIELD_GROUP, INPUT, PANEL, PANEL_COMPACT, RESULT_LINE, SECTION_LABEL,
};

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
    let transfer = ServerAction::<TransferWorkspaceOwnership>::new();
    let assign_pending = assign.pending();
    let remove_pending = remove.pending();
    let transfer_pending = transfer.pending();
    let assign_value = assign.value();
    let remove_value = remove.value();
    let transfer_value = transfer.value();

    let (rows, set_rows) = signal(Vec::<MembershipSummary>::new());
    let (role_options, set_role_options) = signal(Vec::<WorkspaceRoleOption>::new());
    let (current_user_id, set_current_user_id) = signal(None::<String>);
    let (can_transfer_ownership, set_can_transfer_ownership) = signal(false);
    let (requires_step_up, set_requires_step_up) = signal(false);
    let (busy_user_id, set_busy_user_id) = signal(None::<String>);
    let (remove_target, set_remove_target) = signal(None::<(String, String)>);
    let (transfer_open, set_transfer_open) = signal(false);
    let (transfer_target_id, set_transfer_target_id) = signal(String::new());
    let (transfer_confirm, set_transfer_confirm) = signal(String::new());
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
            let owner_cap = ctx
                .capabilities
                .iter()
                .any(|cap| cap == "ownership.transfer")
                || ctx.membership.role_id == "owner";
            set_can_transfer_ownership.set(owner_cap);
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

    Effect::new(move |_| match transfer_value.get() {
        Some(Ok(new_owner)) => {
            set_rows.update(|list| {
                let me = current_user_id.get_untracked();
                for row in list.iter_mut() {
                    if row.user_id == new_owner.user_id {
                        *row = new_owner.clone();
                    } else if me.as_deref() == Some(row.user_id.as_str()) && row.role_id == "owner"
                    {
                        row.role_id = "admin".to_owned();
                    }
                }
            });
            set_transfer_open.set(false);
            set_transfer_target_id.set(String::new());
            set_transfer_confirm.set(String::new());
            set_busy_user_id.set(None);
            set_action_error.set(None);
            set_action_ok.set(Some("Ownership transferred. You are now an admin.".to_owned()));
            set_can_transfer_ownership.set(false);
            reload_members_list(slug.get_untracked(), set_rows, set_action_error);
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

    let any_busy = Memo::new(move |_| {
        assign_pending.get() || remove_pending.get() || transfer_pending.get()
    });

    let transfer_candidates = Memo::new(move |_| {
        let me = current_user_id.get();
        rows.get()
            .into_iter()
            .filter(|m| m.status == "active")
            .filter(|m| me.as_deref() != Some(m.user_id.as_str()))
            .collect::<Vec<_>>()
    });

    let transfer_target_email = Memo::new(move |_| {
        let id = transfer_target_id.get();
        transfer_candidates
            .get()
            .into_iter()
            .find(|m| m.user_id == id)
            .map(|m| {
                if m.primary_email.trim().is_empty() {
                    m.user_id
                } else {
                    m.primary_email
                }
            })
            .unwrap_or_default()
    });

    let transfer_confirm_ok = Memo::new(move |_| {
        let typed = transfer_confirm.get().trim().to_ascii_lowercase();
        let expected = transfer_target_email.get().trim().to_ascii_lowercase();
        !typed.is_empty() && !expected.is_empty() && typed == expected
    });

    view! {
        <Show when=move || members.get().is_none()>
            <p class=RESULT_LINE aria-busy="true">"Loading members…"</p>
        </Show>

        <Show when=move || load_error.get().is_some()>
            <p class=BANNER_ERROR>{move || load_error.get().unwrap_or_default()}</p>
        </Show>

        <Show when=move || requires_step_up.get()>
            <p class="workspace-settings-step-up" role="status">
                "Role changes, removals, and ownership transfer require a step-up session (AAL2). "
                <a href="/account/mfa">"Complete MFA"</a>
                " if your session is not elevated."
            </p>
        </Show>

        <Show when=move || action_error.get().is_some()>
            <p class=BANNER_ERROR>{move || action_error.get().unwrap_or_default()}</p>
        </Show>

        <Show when=move || action_ok.get().is_some()>
            <p class=RESULT_LINE role="status">{move || action_ok.get().unwrap_or_default()}</p>
        </Show>

        <Show when=move || can_transfer_ownership.get() && list_ready.get()>
            <div class="workspace-settings-transfer-bar">
                <p class="board-muted">
                    "As owner you can transfer workspace ownership to another member. "
                    "You become an admin afterward."
                </p>
                <button
                    type="button"
                    class=BTN_SECONDARY
                    disabled=move || any_busy.get() || transfer_candidates.get().is_empty()
                    on:click=move |_| {
                        set_action_error.set(None);
                        set_action_ok.set(None);
                        set_transfer_target_id.set(String::new());
                        set_transfer_confirm.set(String::new());
                        set_transfer_open.set(true);
                    }
                >
                    "Transfer ownership"
                </button>
            </div>
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
                                class=BTN_SECONDARY
                                disabled=move || remove_pending.get()
                                on:click=move |_| set_remove_target.set(None)
                            >
                                "Cancel"
                            </button>
                            <button
                                type="button"
                                class=format!("{}{}", BTN_PRIMARY, " workspace-settings-danger-button")
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

        <Show when=move || transfer_open.get()>
            <div
                class="board-modal-backdrop"
                role="presentation"
                on:click=move |_| {
                    if !transfer_pending.get() {
                        set_transfer_open.set(false);
                    }
                }
            >
                <div
                    class="board-modal vault-modal-confirm"
                    role="dialog"
                    aria-modal="true"
                    aria-labelledby="workspace-transfer-ownership-title"
                    on:click=move |e| e.stop_propagation()
                >
                    <header class="board-modal-head">
                        <div>
                            <h2 id="workspace-transfer-ownership-title">"Transfer ownership"</h2>
                            <p>
                                "The selected member becomes the owner. You are demoted to admin. "
                                "This requires step-up (AAL2)."
                            </p>
                        </div>
                        <button
                            type="button"
                            class="board-modal-close"
                            disabled=move || transfer_pending.get()
                            on:click=move |_| set_transfer_open.set(false)
                        >
                            "Close"
                        </button>
                    </header>
                    <div class="board-modal-body workspace-settings-transfer-modal">
                        <label class=FIELD>
                            <span>"New owner"</span>
                            <select
                                class="workspace-settings-role-select"
                                prop:value=move || transfer_target_id.get()
                                disabled=move || transfer_pending.get()
                                on:change=move |event| {
                                    set_transfer_target_id.set(event_target_value(&event));
                                    set_transfer_confirm.set(String::new());
                                }
                            >
                                <option value="">"Select a member…"</option>
                                {move || {
                                    transfer_candidates
                                        .get()
                                        .into_iter()
                                        .map(|member| {
                                            let id = member.user_id.clone();
                                            let label = if member.primary_email.trim().is_empty() {
                                                format!("{} ({})", member.user_id, member.role_id)
                                            } else {
                                                format!(
                                                    "{} ({})",
                                                    member.primary_email, member.role_id
                                                )
                                            };
                                            let selected = transfer_target_id.get() == id;
                                            view! {
                                                <option value=id selected=selected>
                                                    {label}
                                                </option>
                                            }
                                        })
                                        .collect_view()
                                }}
                            </select>
                        </label>
                        <label class=FIELD>
                            <span>
                                "Type the new owner's email to confirm"
                                {move || {
                                    let email = transfer_target_email.get();
                                    if email.is_empty() {
                                        String::new()
                                    } else {
                                        format!(" ({email})")
                                    }
                                }}
                            </span>
                            <input
                                type="text"
                                autocomplete="off"
                                prop:value=move || transfer_confirm.get()
                                disabled=move || {
                                    transfer_pending.get() || transfer_target_id.get().is_empty()
                                }
                                on:input=move |ev| {
                                    set_transfer_confirm.set(event_target_value(&ev));
                                }
                            />
                        </label>
                        <div class="workspace-settings-modal-actions">
                            <button
                                type="button"
                                class=BTN_SECONDARY
                                disabled=move || transfer_pending.get()
                                on:click=move |_| set_transfer_open.set(false)
                            >
                                "Cancel"
                            </button>
                            <button
                                type="button"
                                class=format!("{}{}", BTN_PRIMARY, " workspace-settings-danger-button")
                                disabled=move || {
                                    !transfer_confirm_ok.get() || transfer_pending.get()
                                }
                                on:click=move |_| {
                                    let slug_value = slug.get_untracked();
                                    let target = transfer_target_id.get_untracked();
                                    if slug_value.is_empty() || target.is_empty() {
                                        set_action_error.set(Some(
                                            "missing workspace or member".to_owned(),
                                        ));
                                        return;
                                    }
                                    set_action_error.set(None);
                                    set_action_ok.set(None);
                                    set_busy_user_id.set(Some(target.clone()));
                                    transfer.dispatch(TransferWorkspaceOwnership {
                                        slug: slug_value,
                                        target_user_id: target,
                                    });
                                }
                            >
                                {move || {
                                    if transfer_pending.get() {
                                        "Transferring…"
                                    } else {
                                        "Transfer ownership"
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
            <td data-label="Email">
                <span class="workspace-settings-member-email">{email}</span>
                {if is_you {
                    view! { <span class="workspace-settings-you-badge">"You"</span> }.into_any()
                } else {
                    view! { <></> }.into_any()
                }}
            </td>
            <td data-label="Role">
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
            <td data-label="Status">{status}</td>
            <td data-label="Joined">{joined}</td>
            <td class="workspace-settings-member-actions" data-label="Actions">
                {if is_you {
                    view! {
                        <span
                            class="board-muted workspace-settings-self-remove-hint"
                            title="Leave from Danger zone"
                        >
                            "Use Danger zone to leave"
                        </span>
                    }
                    .into_any()
                } else {
                    let disabled = actions_locked || row_busy;
                    view! {
                        <button
                            type="button"
                            class=format!("{}{}", BTN_SECONDARY, " workspace-settings-remove-button")
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
