//! Workspace settings — General (editable name, immutable slug).

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use super::shared::{format_settings_timestamp_ms, settings_page_stub, slug_from_settings_pathname};
use crate::app::helpers::{current_browser_pathname, server_error_text};
use crate::app::{
    UpdateWorkspaceName, browser_load, get_workspace_settings_context, update_workspace_name,
};
use leptos::prelude::*;

#[component]
pub fn WorkspaceSettingsGeneralPage() -> impl IntoView {
    settings_page_stub(
        "General",
        "Workspace name and URL. The slug is fixed after create.",
        "Requires organization.view. Mutations require organization.update and step-up (AAL2).",
        view! { <WorkspaceSettingsGeneralBody /> },
    )
}

/// Island: load settings context by slug; edit display name; slug read-only.
#[island]
pub fn WorkspaceSettingsGeneralBody() -> impl IntoView {
    let slug = Memo::new(move |_| slug_from_settings_pathname(&current_browser_pathname()));

    let context = browser_load({
        move || {
            let slug = slug_from_settings_pathname(&current_browser_pathname());
            get_workspace_settings_context(slug)
        }
    });

    let save = ServerAction::<UpdateWorkspaceName>::new();
    let pending = save.pending();
    let save_value = save.value();

    let (name, set_name) = signal(String::new());
    let (original_name, set_original_name) = signal(String::new());
    let (seeded, set_seeded) = signal(false);
    let (client_error, set_client_error) = signal(None::<String>);
    let (show_success, set_show_success) = signal(false);

    Effect::new(move |_| {
        if seeded.get() {
            return;
        }
        if let Some(Ok(ctx)) = context.get() {
            set_name.set(ctx.organization.name.clone());
            set_original_name.set(ctx.organization.name.clone());
            set_seeded.set(true);
        }
    });

    Effect::new(move |_| match save_value.get() {
        Some(Ok(summary)) => {
            set_name.set(summary.name.clone());
            set_original_name.set(summary.name.clone());
            set_client_error.set(None);
            set_show_success.set(true);
        }
        Some(Err(error)) => {
            set_show_success.set(false);
            set_client_error.set(Some(server_error_text(error)));
        }
        None => {}
    });

    let load_error = Memo::new(move |_| {
        context.get().and_then(|result| match result {
            Ok(_) => None,
            Err(error) => Some(server_error_text(error)),
        })
    });

    let step_up_hint = Memo::new(move |_| {
        context
            .get()
            .and_then(|result| result.ok())
            .map(|ctx| ctx.requires_step_up)
            .unwrap_or(false)
    });

    let name_dirty = Memo::new(move |_| {
        name.get().trim() != original_name.get().trim()
    });

    let can_save = Memo::new(move |_| {
        !pending.get() && !name.get().trim().is_empty() && name_dirty.get()
    });

    view! {
        <Show when=move || context.get().is_none()>
            <p class="result-line" aria-busy="true">"Loading workspace…"</p>
        </Show>

        <Show when=move || load_error.get().is_some()>
            <p class="error-banner">{move || load_error.get().unwrap_or_default()}</p>
        </Show>

        <Show when=move || context.get().and_then(|r| r.ok()).is_some()>
            <form
                class="workspace-settings-general-form"
                on:submit=move |event| {
                    event.prevent_default();
                    let slug_value = slug.get_untracked();
                    let name_value = name.get_untracked().trim().to_owned();
                    if slug_value.is_empty() || name_value.is_empty() {
                        set_client_error.set(Some("workspace name is required".to_owned()));
                        set_show_success.set(false);
                        return;
                    }
                    if name_value == original_name.get_untracked().trim() {
                        return;
                    }
                    set_client_error.set(None);
                    set_show_success.set(false);
                    save.dispatch(UpdateWorkspaceName {
                        slug: slug_value,
                        name: name_value,
                    });
                }
            >
                <label class="auth-field">
                    <span>"Display name"</span>
                    <input
                        class="auth-input"
                        type="text"
                        maxlength="120"
                        autocomplete="organization"
                        prop:value=move || name.get()
                        on:input=move |event| {
                            set_name.set(event_target_value(&event));
                            set_show_success.set(false);
                        }
                        disabled=move || pending.get()
                    />
                </label>

                <dl class="kv workspace-settings-kv">
                    <dt>"Workspace URL"</dt>
                    <dd class="mono-value">
                        {move || {
                            let s = slug.get();
                            if s.is_empty() {
                                "—".to_owned()
                            } else {
                                format!("/org/{s}")
                            }
                        }}
                    </dd>
                    <dt>"Slug"</dt>
                    <dd class="mono-value">
                        {move || {
                            let s = slug.get();
                            if s.is_empty() { "—".to_owned() } else { s }
                        }}
                        <span class="workspace-settings-readonly-tag">" read-only"</span>
                    </dd>
                    {move || {
                        context.get().and_then(|r| r.ok()).map(|ctx| {
                            let status = if ctx.organization.status.trim().is_empty() {
                                "—".to_owned()
                            } else {
                                ctx.organization.status.clone()
                            };
                            let created = format_settings_timestamp_ms(ctx.organization.created_at_ms);
                            view! {
                                <dt>"Status"</dt>
                                <dd>{status}</dd>
                                <dt>"Created"</dt>
                                <dd>{created}</dd>
                            }
                            .into_any()
                        })
                        .unwrap_or_else(|| view! { <></> }.into_any())
                    }}
                </dl>

                <Show when=move || step_up_hint.get()>
                    <p class="workspace-settings-step-up" role="status">
                        "Saving the name requires a step-up session (AAL2). "
                        <a href="/account/mfa">"Complete MFA"</a>
                        " if your session is not elevated, then try again."
                    </p>
                </Show>

                <Show when=move || client_error.get().is_some()>
                    <p class="error-banner">{move || client_error.get().unwrap_or_default()}</p>
                </Show>

                <Show when=move || show_success.get()>
                    <p class="result-line" role="status">"Workspace name saved."</p>
                </Show>

                <div class="actions">
                    <button
                        type="submit"
                        class="link-button link-button-primary"
                        disabled=move || !can_save.get()
                    >
                        {move || if pending.get() { "Saving…" } else { "Save name" }}
                    </button>
                </div>
            </form>
        </Show>
    }
}
