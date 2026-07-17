//! Create-organization modal for the workspaces switcher.

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use crate::app::helpers::action_result_text;
use crate::app::{CreateOrganization, create_organization};
use crate::ui::classes::{
    BANNER_ERROR, BTN_PRIMARY, BTN_SECONDARY, FIELD, FIELD_HINT, INPUT, MONO_VALUE,
    ORG_CREATE_ACTIONS, ORG_CREATE_BACKDROP, ORG_CREATE_BODY, ORG_CREATE_CLOSE, ORG_CREATE_FIELDS,
    ORG_CREATE_FORM, ORG_CREATE_HEAD, ORG_CREATE_HEAD_P, ORG_CREATE_HEAD_TITLE, ORG_CREATE_KICKER,
    ORG_CREATE_MODAL, SLUG_INPUT_FIELD, SLUG_INPUT_GROUP, SLUG_INPUT_PREFIX, with_extra,
};
use leptos::prelude::*;

/// Modal dialog for creating a new organization (workspace).
///
/// Owns create form state and the `CreateOrganization` server action. Parent
/// controls open/close via `open` / `set_open`.
#[component]
pub fn CreateOrganizationModal(
    open: ReadSignal<bool>,
    set_open: WriteSignal<bool>,
) -> impl IntoView {
    let create_action = ServerAction::<CreateOrganization>::new();
    let create_pending = create_action.pending();
    let create_value = create_action.value();
    let (name, set_name) = signal(String::new());
    let (slug, set_slug) = signal(String::new());
    let (slug_touched, set_slug_touched) = signal(false);

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
            set_open.set(false);
            #[cfg(feature = "hydrate")]
            {
                // Refresh the list after a successful create.
                if let Some(window) = web_sys::window() {
                    let _ = window.location().reload();
                }
            }
        }
    });

    // Lock document scroll while open (shared class with dashboard board modals).
    Effect::new(move |_| {
        let is_open = open.get();
        #[cfg(feature = "hydrate")]
        {
            if let Some(document) = web_sys::window().and_then(|window| window.document())
                && let Some(root) = document.document_element()
            {
                let _ = if is_open {
                    root.class_list().add_1("board-modal-open")
                } else {
                    root.class_list().remove_1("board-modal-open")
                };
            }
        }
        #[cfg(not(feature = "hydrate"))]
        {
            let _ = is_open;
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

    let slug_input_class = with_extra(
        &with_extra(INPUT, Some(SLUG_INPUT_FIELD)),
        Some(MONO_VALUE),
    );

    view! {
        <Show when=move || open.get()>
            <div
                class=ORG_CREATE_BACKDROP
                role="presentation"
                tabindex="-1"
                on:click=move |_| {
                    if !create_pending.get_untracked() {
                        set_open.set(false);
                    }
                }
                on:keydown=move |event| {
                    if event.key() == "Escape" && !create_pending.get_untracked() {
                        set_open.set(false);
                    }
                }
                on:wheel=move |event| event.stop_propagation()
            >
                <div
                    class=ORG_CREATE_MODAL
                    role="dialog"
                    aria-modal="true"
                    aria-labelledby="org-create-title"
                    aria-describedby="org-create-description"
                    on:click=move |event| event.stop_propagation()
                >
                    <header class=ORG_CREATE_HEAD>
                        <div>
                            <p class=ORG_CREATE_KICKER>"New workspace"</p>
                            <h2 id="org-create-title" class=ORG_CREATE_HEAD_TITLE>"Create organization"</h2>
                            <p id="org-create-description" class=ORG_CREATE_HEAD_P>
                                "Choose a recognizable name and URL. You will become the owner."
                            </p>
                        </div>
                        <button
                            type="button"
                            class=ORG_CREATE_CLOSE
                            disabled=move || create_pending.get()
                            on:click=move |_| set_open.set(false)
                        >
                            "Close"
                        </button>
                    </header>
                    <div class=ORG_CREATE_BODY>
                        <form
                            class=ORG_CREATE_FORM
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
                            <div class=ORG_CREATE_FIELDS>
                                <label class=FIELD>
                                    <span>"Organization name"</span>
                                    <input
                                        class=INPUT
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
                                    <small class=FIELD_HINT>"Use the name teammates will recognize in the workspace switcher."</small>
                                </label>
                                <label class=FIELD>
                                    <span>"Workspace URL"</span>
                                    <div class=SLUG_INPUT_GROUP role="group" aria-label="Workspace URL">
                                        <span class=SLUG_INPUT_PREFIX aria-hidden="true">"/org/"</span>
                                        <input
                                            class=slug_input_class.clone()
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
                                    <small class=FIELD_HINT>"Lowercase letters, numbers, and hyphens only."</small>
                                </label>
                            </div>
                            <Show when=move || {
                                create_value.get().is_some_and(|result| result.is_err())
                            }>
                                <p class=BANNER_ERROR>{move || action_result_text(create_value.get())}</p>
                            </Show>
                            <div class=ORG_CREATE_ACTIONS>
                                <button
                                    type="button"
                                    class=BTN_SECONDARY
                                    disabled=move || create_pending.get()
                                    on:click=move |_| set_open.set(false)
                                >
                                    "Cancel"
                                </button>
                                <button
                                    type="submit"
                                    class=BTN_PRIMARY
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
    }
}
