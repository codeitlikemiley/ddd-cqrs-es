//! Workspace settings — Invitations (invite form, list, resend, revoke).

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use super::shared::{
    display_role_name, format_settings_timestamp_ms, role_label_from_options, settings_page_stub,
    slug_from_settings_pathname,
};
use crate::app::helpers::{current_browser_pathname, server_error_text};
use crate::app::{
    InviteWorkspaceMember, ResendWorkspaceInvitation, RevokeWorkspaceInvitation, browser_load,
    get_workspace_settings_context, list_workspace_invitations,
};
use crate::contracts::{InvitationSummary, WorkspaceRoleOption};
use leptos::prelude::*;
#[cfg(feature = "hydrate")]
use leptos::task::spawn_local;

#[component]
pub fn WorkspaceSettingsInvitationsPage() -> impl IntoView {
    settings_page_stub(
        "Invitations",
        "Pending invites to join this workspace.",
        "Requires member.view. Invite, resend, and revoke need member.invite and step-up (AAL2).",
        view! { <WorkspaceSettingsInvitationsBody /> },
    )
}

/// Island: invite form + invitation table with resend/revoke for pending rows.
#[island]
pub fn WorkspaceSettingsInvitationsBody() -> impl IntoView {
    let slug = Memo::new(move |_| slug_from_settings_pathname(&current_browser_pathname()));

    let invitations = browser_load({
        move || {
            let slug = slug_from_settings_pathname(&current_browser_pathname());
            list_workspace_invitations(slug)
        }
    });
    let context = browser_load({
        move || {
            let slug = slug_from_settings_pathname(&current_browser_pathname());
            get_workspace_settings_context(slug)
        }
    });

    let invite = ServerAction::<InviteWorkspaceMember>::new();
    let resend = ServerAction::<ResendWorkspaceInvitation>::new();
    let revoke = ServerAction::<RevokeWorkspaceInvitation>::new();
    let invite_pending = invite.pending();
    let resend_pending = resend.pending();
    let revoke_pending = revoke.pending();
    let invite_value = invite.value();
    let resend_value = resend.value();
    let revoke_value = revoke.value();

    let (rows, set_rows) = signal(Vec::<InvitationSummary>::new());
    let (role_options, set_role_options) = signal(Vec::<WorkspaceRoleOption>::new());
    let (requires_step_up, set_requires_step_up) = signal(false);
    let (email, set_email) = signal(String::new());
    let (role_id, set_role_id) = signal(String::new());
    let (role_seeded, set_role_seeded) = signal(false);
    let (busy_id, set_busy_id) = signal(None::<String>);
    let (action_error, set_action_error) = signal(None::<String>);
    let (action_ok, set_action_ok) = signal(None::<String>);
    let (list_ready, set_list_ready) = signal(false);

    Effect::new(move |_| {
        if let Some(Ok(list)) = invitations.get() {
            set_rows.set(list.invitations);
            set_list_ready.set(true);
        }
    });

    Effect::new(move |_| {
        if let Some(Ok(ctx)) = context.get() {
            set_role_options.set(ctx.role_options.clone());
            set_requires_step_up.set(ctx.requires_step_up);
            if !role_seeded.get_untracked() {
                let default = ctx
                    .role_options
                    .iter()
                    .find(|opt| opt.role_id == "member")
                    .or_else(|| ctx.role_options.first())
                    .map(|opt| opt.role_id.clone())
                    .unwrap_or_default();
                set_role_id.set(default);
                set_role_seeded.set(true);
            }
        }
    });

    Effect::new(move |_| match invite_value.get() {
        Some(Ok(created)) => {
            set_rows.update(|list| {
                list.retain(|row| {
                    !(row.email == created.email && row.status == "pending" && row.invitation_id != created.invitation_id)
                });
                if let Some(existing) = list
                    .iter_mut()
                    .find(|row| row.invitation_id == created.invitation_id)
                {
                    *existing = created;
                } else {
                    list.insert(0, created);
                }
            });
            set_email.set(String::new());
            set_busy_id.set(None);
            set_action_error.set(None);
            set_action_ok.set(Some("Invitation sent.".to_owned()));
        }
        Some(Err(error)) => {
            set_busy_id.set(None);
            set_action_ok.set(None);
            set_action_error.set(Some(server_error_text(error)));
        }
        None => {}
    });

    Effect::new(move |_| match resend_value.get() {
        Some(Ok(updated)) => {
            set_rows.update(|list| {
                if let Some(row) = list
                    .iter_mut()
                    .find(|row| row.invitation_id == updated.invitation_id)
                {
                    *row = updated;
                }
            });
            set_busy_id.set(None);
            set_action_error.set(None);
            set_action_ok.set(Some("Invitation resent.".to_owned()));
        }
        Some(Err(error)) => {
            set_busy_id.set(None);
            set_action_ok.set(None);
            set_action_error.set(Some(server_error_text(error)));
            reload_invitations_list(slug.get_untracked(), set_rows, set_action_error);
        }
        None => {}
    });

    Effect::new(move |_| match revoke_value.get() {
        Some(Ok(updated)) => {
            set_rows.update(|list| {
                if let Some(row) = list
                    .iter_mut()
                    .find(|row| row.invitation_id == updated.invitation_id)
                {
                    *row = updated;
                }
            });
            set_busy_id.set(None);
            set_action_error.set(None);
            set_action_ok.set(Some("Invitation revoked.".to_owned()));
        }
        Some(Err(error)) => {
            set_busy_id.set(None);
            set_action_ok.set(None);
            set_action_error.set(Some(server_error_text(error)));
        }
        None => {}
    });

    let load_error = Memo::new(move |_| {
        invitations.get().and_then(|result| match result {
            Ok(_) => None,
            Err(error) => Some(server_error_text(error)),
        })
    });

    let any_busy = Memo::new(move |_| {
        invite_pending.get() || resend_pending.get() || revoke_pending.get()
    });

    let can_invite = Memo::new(move |_| {
        !any_busy.get()
            && !email.get().trim().is_empty()
            && !role_id.get().trim().is_empty()
            && !slug.get().is_empty()
    });

    view! {
        <Show when=move || invitations.get().is_none()>
            <p class="result-line" aria-busy="true">"Loading invitations…"</p>
        </Show>

        <Show when=move || load_error.get().is_some()>
            <p class="error-banner">{move || load_error.get().unwrap_or_default()}</p>
        </Show>

        <Show when=move || requires_step_up.get()>
            <p class="workspace-settings-step-up" role="status">
                "Invite, resend, and revoke require a step-up session (AAL2). "
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

        <form
            class="workspace-settings-invite-form"
            on:submit=move |event| {
                event.prevent_default();
                let slug_value = slug.get_untracked();
                let email_value = email.get_untracked().trim().to_owned();
                let role_value = role_id.get_untracked();
                if slug_value.is_empty() || email_value.is_empty() || role_value.is_empty() {
                    set_action_error.set(Some("email and role are required".to_owned()));
                    set_action_ok.set(None);
                    return;
                }
                if !role_options
                    .get_untracked()
                    .iter()
                    .any(|opt| opt.role_id == role_value)
                {
                    set_action_error.set(Some("role is not available for invite".to_owned()));
                    set_action_ok.set(None);
                    return;
                }
                set_action_error.set(None);
                set_action_ok.set(None);
                set_busy_id.set(Some("__invite__".to_owned()));
                invite.dispatch(InviteWorkspaceMember {
                    slug: slug_value,
                    email: email_value,
                    role_id: role_value,
                });
            }
        >
            <label class="auth-field">
                <span>"Email"</span>
                <input
                    class="auth-input"
                    type="email"
                    autocomplete="email"
                    maxlength="320"
                    prop:value=move || email.get()
                    on:input=move |event| {
                        set_email.set(event_target_value(&event));
                        set_action_ok.set(None);
                    }
                    disabled=move || any_busy.get()
                />
            </label>
            <label class="auth-field">
                <span>"Role"</span>
                <select
                    class="workspace-settings-role-select"
                    prop:value=move || role_id.get()
                    disabled=move || any_busy.get() || role_options.get().is_empty()
                    on:change=move |event| {
                        set_role_id.set(event_target_value(&event));
                        set_action_ok.set(None);
                    }
                >
                    {move || {
                        role_options
                            .get()
                            .into_iter()
                            .map(|opt| {
                                let id = opt.role_id.clone();
                                let label = if opt.name.trim().is_empty() {
                                    display_role_name(&opt.role_id)
                                } else {
                                    opt.name.clone()
                                };
                                let selected = id == role_id.get();
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
            <div class="workspace-settings-invite-actions">
                <button
                    type="submit"
                    class="primary-button"
                    disabled=move || !can_invite.get()
                >
                    {move || {
                        if invite_pending.get() {
                            "Sending…"
                        } else {
                            "Send invitation"
                        }
                    }}
                </button>
            </div>
        </form>

        {move || {
            if !list_ready.get() {
                return view! { <></> }.into_any();
            }
            let list = rows.get();
            if list.is_empty() {
                return view! {
                    <div class="workspace-settings-empty" role="status">
                        <p>"No invitations yet."</p>
                        <p class="board-muted">
                            "Send an invite above. Only hashes are stored; the one-time link is mailed."
                        </p>
                    </div>
                }
                .into_any();
            }

            let options = role_options.get();
            let busy = busy_id.get();
            let actions_locked = any_busy.get();

            view! {
                <div class="table-wrap workspace-settings-table-wrap">
                    <table class="data-table workspace-settings-invitations-table">
                        <thead>
                            <tr>
                                <th scope="col">"Email"</th>
                                <th scope="col">"Role"</th>
                                <th scope="col">"Status"</th>
                                <th scope="col">"Expires"</th>
                                <th scope="col">"Actions"</th>
                            </tr>
                        </thead>
                        <tbody>
                            {list
                                .into_iter()
                                .map(|invitation| {
                                    invitation_row(
                                        invitation,
                                        options.clone(),
                                        busy.clone(),
                                        actions_locked,
                                        slug,
                                        resend,
                                        revoke,
                                        set_busy_id,
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
    }
}

fn invitation_row(
    invitation: InvitationSummary,
    role_options: Vec<WorkspaceRoleOption>,
    busy_id: Option<String>,
    actions_locked: bool,
    slug: Memo<String>,
    resend: ServerAction<ResendWorkspaceInvitation>,
    revoke: ServerAction<RevokeWorkspaceInvitation>,
    set_busy_id: WriteSignal<Option<String>>,
    set_action_error: WriteSignal<Option<String>>,
    set_action_ok: WriteSignal<Option<String>>,
) -> impl IntoView {
    let invitation_id = invitation.invitation_id.clone();
    let invitation_id_resend = invitation_id.clone();
    let invitation_id_revoke = invitation_id.clone();
    let invitation_id_busy = invitation_id.clone();
    let email = if invitation.email.trim().is_empty() {
        "—".to_owned()
    } else {
        invitation.email.clone()
    };
    let status = invitation.status.clone();
    let expires = format_settings_timestamp_ms(invitation.expires_at_ms);
    let role_label = role_label_from_options(&invitation.role_id, &role_options);
    let is_pending = status == "pending";
    let row_busy = busy_id.as_deref() == Some(invitation_id.as_str());
    let disabled = actions_locked || row_busy || !is_pending;

    view! {
        <tr class="workspace-settings-invitation-row">
            <td>
                <span class="workspace-settings-member-email">{email}</span>
            </td>
            <td>{role_label}</td>
            <td>
                <span class=format!(
                    "workspace-settings-status-pill is-{}",
                    status
                )>{status.clone()}</span>
            </td>
            <td>{expires}</td>
            <td class="workspace-settings-member-actions">
                {if is_pending {
                    view! {
                        <div class="workspace-settings-invitation-actions">
                            <button
                                type="button"
                                class="secondary-button"
                                disabled=disabled
                                on:click=move |_| {
                                    let slug_value = slug.get_untracked();
                                    if slug_value.is_empty() {
                                        set_action_error.set(Some("missing workspace".to_owned()));
                                        return;
                                    }
                                    set_action_error.set(None);
                                    set_action_ok.set(None);
                                    set_busy_id.set(Some(invitation_id_resend.clone()));
                                    resend.dispatch(ResendWorkspaceInvitation {
                                        slug: slug_value,
                                        invitation_id: invitation_id_resend.clone(),
                                    });
                                }
                            >
                                {if busy_id.as_deref() == Some(invitation_id_busy.as_str())
                                    && actions_locked
                                {
                                    "Working…"
                                } else {
                                    "Resend"
                                }}
                            </button>
                            <button
                                type="button"
                                class="secondary-button workspace-settings-remove-button"
                                disabled=disabled
                                on:click=move |_| {
                                    let slug_value = slug.get_untracked();
                                    if slug_value.is_empty() {
                                        set_action_error.set(Some("missing workspace".to_owned()));
                                        return;
                                    }
                                    set_action_error.set(None);
                                    set_action_ok.set(None);
                                    set_busy_id.set(Some(invitation_id_revoke.clone()));
                                    revoke.dispatch(RevokeWorkspaceInvitation {
                                        slug: slug_value,
                                        invitation_id: invitation_id_revoke.clone(),
                                    });
                                }
                            >
                                "Revoke"
                            </button>
                        </div>
                    }
                    .into_any()
                } else {
                    view! { <span class="board-muted">"—"</span> }.into_any()
                }}
            </td>
        </tr>
    }
}

fn reload_invitations_list(
    slug: String,
    set_rows: WriteSignal<Vec<InvitationSummary>>,
    set_action_error: WriteSignal<Option<String>>,
) {
    if slug.is_empty() {
        return;
    }
    #[cfg(feature = "hydrate")]
    {
        spawn_local(async move {
            match list_workspace_invitations(slug).await {
                Ok(list) => set_rows.set(list.invitations),
                Err(error) => set_action_error.set(Some(server_error_text(error))),
            }
        });
    }
    #[cfg(not(feature = "hydrate"))]
    {
        let _ = (set_rows, set_action_error);
    }
}
