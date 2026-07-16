//! Workspace settings — Leave workspace and soft-deactivate (danger zone).

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use super::shared::{settings_page_stub, slug_from_settings_pathname};
use crate::app::helpers::{current_browser_pathname, redirect_browser, server_error_text};
use crate::app::{
    DeactivateWorkspace, LeaveWorkspace, browser_load, deactivate_workspace,
    get_workspace_settings_context, leave_workspace,
};
use crate::ui::classes::{
    BANNER_ERROR, BTN_PRIMARY, BTN_SECONDARY, FIELD, INPUT, MUTED, RESULT_LINE,
    VAULT_MODAL_BACKDROP, VAULT_MODAL_BODY, VAULT_MODAL_CLOSE, VAULT_MODAL_CONFIRM,
    VAULT_MODAL_HEAD, VAULT_MODAL_HEAD_P, VAULT_MODAL_HEAD_TITLE, WS_DANGER_BUTTON, WS_DANGER_CARD,
    WS_DANGER_CARD_BTN, WS_DANGER_CONFIRM, WS_DANGER_ZONES, WS_MODAL_ACTIONS, WS_STEP_UP,
    with_extra,
};
use leptos::prelude::*;

#[component]
pub fn WorkspaceSettingsDangerPage() -> impl IntoView {
    settings_page_stub(
        "Danger zone",
        "Leave this workspace or soft-deactivate it. There is no hard delete.",
        "Leave is available to any member (except the last owner). Deactivate requires ownership and step-up (AAL2).",
        view! { <WorkspaceSettingsDangerBody /> },
    )
}

/// Island: leave workspace + soft deactivate with typed confirmation.
#[island]
pub fn WorkspaceSettingsDangerBody() -> impl IntoView {
    let slug = Memo::new(move |_| slug_from_settings_pathname(&current_browser_pathname()));

    let context = browser_load({
        move || {
            let slug = slug_from_settings_pathname(&current_browser_pathname());
            get_workspace_settings_context(slug)
        }
    });

    let leave = ServerAction::<LeaveWorkspace>::new();
    let deactivate = ServerAction::<DeactivateWorkspace>::new();
    let leave_pending = leave.pending();
    let deactivate_pending = deactivate.pending();
    let leave_value = leave.value();
    let deactivate_value = deactivate.value();

    let (workspace_name, set_workspace_name) = signal(String::new());
    let (workspace_slug, set_workspace_slug) = signal(String::new());
    let (role_id, set_role_id) = signal(String::new());
    let (capabilities, set_capabilities) = signal(Vec::<String>::new());
    let (requires_step_up, set_requires_step_up) = signal(false);
    let (leave_confirm_open, set_leave_confirm_open) = signal(false);
    let (deactivate_confirm, set_deactivate_confirm) = signal(String::new());
    let (action_error, set_action_error) = signal(None::<String>);
    let (action_ok, set_action_ok) = signal(None::<String>);
    let (ready, set_ready) = signal(false);

    Effect::new(move |_| {
        if let Some(Ok(ctx)) = context.get() {
            set_workspace_name.set(ctx.organization.name.clone());
            set_workspace_slug.set(ctx.organization.slug.clone());
            set_role_id.set(ctx.membership.role_id.clone());
            set_capabilities.set(ctx.capabilities.clone());
            set_requires_step_up.set(ctx.requires_step_up);
            set_ready.set(true);
        }
    });

    Effect::new(move |_| match leave_value.get() {
        Some(Ok(_)) => {
            set_leave_confirm_open.set(false);
            set_action_error.set(None);
            set_action_ok.set(Some("You left the workspace.".to_owned()));
            redirect_browser("/organizations");
        }
        Some(Err(error)) => {
            set_action_ok.set(None);
            set_action_error.set(Some(server_error_text(error)));
        }
        None => {}
    });

    Effect::new(move |_| match deactivate_value.get() {
        Some(Ok(_)) => {
            set_action_error.set(None);
            set_action_ok.set(Some("Workspace deactivated.".to_owned()));
            redirect_browser("/organizations");
        }
        Some(Err(error)) => {
            set_action_ok.set(None);
            set_action_error.set(Some(server_error_text(error)));
        }
        None => {}
    });

    let load_error = Memo::new(move |_| {
        context.get().and_then(|result| match result {
            Ok(_) => None,
            Err(error) => Some(server_error_text(error)),
        })
    });

    let can_deactivate = Memo::new(move |_| {
        capabilities
            .get()
            .iter()
            .any(|cap| cap == "ownership.transfer")
            || role_id.get() == "owner"
    });

    let deactivate_match = Memo::new(move |_| {
        let typed = deactivate_confirm.get().trim().to_ascii_lowercase();
        if typed.is_empty() {
            return false;
        }
        let name = workspace_name.get().trim().to_ascii_lowercase();
        let slug_value = workspace_slug.get().trim().to_ascii_lowercase();
        typed == name || (!slug_value.is_empty() && typed == slug_value)
    });

    view! {
        <Show when=move || context.get().is_none()>
            <p class=RESULT_LINE aria-busy="true">"Loading danger zone…"</p>
        </Show>

        <Show when=move || load_error.get().is_some()>
            <p class=BANNER_ERROR>{move || load_error.get().unwrap_or_default()}</p>
        </Show>

        <Show when=move || requires_step_up.get() && can_deactivate.get()>
            <p class=WS_STEP_UP role="status">
                "Deactivating a workspace requires a step-up session (AAL2). "
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

        <Show when=move || ready.get()>
            <div class=WS_DANGER_ZONES>
                <section class=WS_DANGER_CARD aria-labelledby="leave-workspace-title">
                    <h3 id="leave-workspace-title">"Leave workspace"</h3>
                    <p class=MUTED>
                        "Remove yourself from this workspace. You will lose access immediately. "
                        "The last owner cannot leave — transfer ownership first."
                    </p>
                    <button
                        type="button"
                        class=with_extra(
                            BTN_SECONDARY,
                            Some(&format!("{WS_DANGER_BUTTON} {WS_DANGER_CARD_BTN}")),
                        )
                        disabled=move || leave_pending.get() || deactivate_pending.get()
                        on:click=move |_| {
                            set_action_error.set(None);
                            set_action_ok.set(None);
                            set_leave_confirm_open.set(true);
                        }
                    >
                        "Leave workspace"
                    </button>
                </section>

                <section
                    class=WS_DANGER_CARD
                    aria-labelledby="deactivate-workspace-title"
                >
                    <h3 id="deactivate-workspace-title">"Deactivate workspace"</h3>
                    <p class=MUTED>
                        "Soft-deactivates this workspace (status archived). Pending invitations are revoked "
                        "and members can no longer select it. "
                        <strong>"There is no hard delete"</strong>
                        " — data is retained for recovery and audit. Only owners can deactivate."
                    </p>
                    <Show when=move || !can_deactivate.get()>
                        <p class=MUTED role="status">
                            "Only workspace owners can deactivate this workspace."
                        </p>
                    </Show>
                    <Show when=move || can_deactivate.get()>
                        <div class=WS_DANGER_CONFIRM>
                            <label class=FIELD>
                                <span>
                                    "Type the workspace name or slug ("
                                    <code>{move || {
                                        let name = workspace_name.get();
                                        let slug_value = workspace_slug.get();
                                        if slug_value.is_empty() {
                                            name
                                        } else {
                                            format!("{name} / {slug_value}")
                                        }
                                    }}</code>
                                    ") to enable deactivation"
                                </span>
                                <input
                                    class=INPUT
                                    type="text"
                                    autocomplete="off"
                                    prop:value=move || deactivate_confirm.get()
                                    on:input=move |ev| {
                                        set_deactivate_confirm.set(event_target_value(&ev));
                                    }
                                />
                            </label>
                            <button
                                type="button"
                                class=with_extra(
                                    BTN_PRIMARY,
                                    Some(&format!("{WS_DANGER_BUTTON} {WS_DANGER_CARD_BTN}")),
                                )
                                disabled=move || {
                                    !deactivate_match.get()
                                        || deactivate_pending.get()
                                        || leave_pending.get()
                                }
                                on:click=move |_| {
                                    let slug_value = slug.get_untracked();
                                    if slug_value.is_empty() {
                                        set_action_error.set(Some("missing workspace".to_owned()));
                                        return;
                                    }
                                    set_action_error.set(None);
                                    set_action_ok.set(None);
                                    deactivate.dispatch(DeactivateWorkspace {
                                        slug: slug_value,
                                    });
                                }
                            >
                                {move || {
                                    if deactivate_pending.get() {
                                        "Deactivating…"
                                    } else {
                                        "Deactivate workspace"
                                    }
                                }}
                            </button>
                        </div>
                    </Show>
                </section>
            </div>
        </Show>

        <Show when=move || leave_confirm_open.get()>
            <div
                class=VAULT_MODAL_BACKDROP
                role="presentation"
                on:click=move |_| {
                    if !leave_pending.get() {
                        set_leave_confirm_open.set(false);
                    }
                }
            >
                <div
                    class=VAULT_MODAL_CONFIRM
                    role="dialog"
                    aria-modal="true"
                    aria-labelledby="workspace-leave-title"
                    on:click=move |e| e.stop_propagation()
                >
                    <header class=VAULT_MODAL_HEAD>
                        <div>
                            <h2 id="workspace-leave-title" class=VAULT_MODAL_HEAD_TITLE>"Leave this workspace?"</h2>
                            <p class=VAULT_MODAL_HEAD_P>
                                "You will immediately lose access to "
                                <strong>{move || workspace_name.get()}</strong>
                                ". The last owner cannot leave without transferring ownership first."
                            </p>
                        </div>
                        <button
                            type="button"
                            class=VAULT_MODAL_CLOSE
                            disabled=move || leave_pending.get()
                            on:click=move |_| set_leave_confirm_open.set(false)
                        >
                            "Close"
                        </button>
                    </header>
                    <div class=VAULT_MODAL_BODY>
                        <div class=WS_MODAL_ACTIONS>
                            <button
                                type="button"
                                class=BTN_SECONDARY
                                disabled=move || leave_pending.get()
                                on:click=move |_| set_leave_confirm_open.set(false)
                            >
                                "Cancel"
                            </button>
                            <button
                                type="button"
                                class=with_extra(BTN_PRIMARY, Some(WS_DANGER_BUTTON))
                                disabled=move || leave_pending.get()
                                on:click=move |_| {
                                    let slug_value = slug.get_untracked();
                                    if slug_value.is_empty() {
                                        set_action_error.set(Some("missing workspace".to_owned()));
                                        return;
                                    }
                                    set_action_error.set(None);
                                    set_action_ok.set(None);
                                    leave.dispatch(LeaveWorkspace {
                                        slug: slug_value,
                                    });
                                }
                            >
                                {move || {
                                    if leave_pending.get() {
                                        "Leaving…"
                                    } else {
                                        "Leave workspace"
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
