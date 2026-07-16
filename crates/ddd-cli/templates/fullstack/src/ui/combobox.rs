//! Accessible filter combobox: type to filter, arrow keys, Enter/Tab to select.

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]

use leptos::prelude::*;
#[cfg(feature = "hydrate")]
use web_sys::KeyboardEvent;

use super::classes::{
    COMBOBOX, COMBOBOX_CHEVRON, COMBOBOX_CONTROL, COMBOBOX_EMPTY, COMBOBOX_INPUT, COMBOBOX_LABEL,
    COMBOBOX_LIST, COMBOBOX_OPTION, COMBOBOX_OPTION_ACTIVE, COMBOBOX_OPTION_SELECTED,
};

/// One option in a filter combobox (`value` is the stored key; `label` is shown).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComboboxOption {
    pub value: String,
    pub label: String,
}

/// Typeahead combobox for filter toolbars (Action / Outcome, etc.).
#[component]
pub fn FilterCombobox(
    /// Visible field label above the control.
    #[prop(into)]
    label: String,
    /// Empty-value label (e.g. "All actions").
    #[prop(into)]
    all_label: String,
    /// Available options (raw value + display label).
    options: Signal<Vec<ComboboxOption>>,
    /// Currently selected raw value (`""` means “all”).
    value: RwSignal<String>,
    #[prop(optional, into)]
    disabled: Signal<bool>,
    #[prop(optional, into)]
    placeholder: Option<String>,
) -> impl IntoView {
    let (open, set_open) = signal(false);
    let (query, set_query) = signal(String::new());
    let (highlight, set_highlight) = signal(0usize);
    let (typing, set_typing) = signal(false);

    let selected_label = Memo::new({
        let all_label = all_label.clone();
        move |_| {
            let current = value.get();
            if current.is_empty() {
                return all_label.clone();
            }
            options
                .get()
                .into_iter()
                .find(|opt| opt.value == current)
                .map(|opt| opt.label)
                .unwrap_or(current)
        }
    });

    let display_value = Memo::new(move |_| {
        if typing.get() {
            query.get()
        } else {
            selected_label.get()
        }
    });

    let filtered = Memo::new({
        let all_label = all_label.clone();
        move |_| {
            let q = query.get().trim().to_ascii_lowercase();
            let mut items = Vec::new();
            items.push(ComboboxOption {
                value: String::new(),
                label: all_label.clone(),
            });
            for opt in options.get() {
                if q.is_empty()
                    || opt.label.to_ascii_lowercase().contains(&q)
                    || opt.value.to_ascii_lowercase().contains(&q)
                {
                    items.push(opt);
                }
            }
            items
        }
    });

    let commit = move |next: String| {
        value.set(next);
        set_typing.set(false);
        set_query.set(String::new());
        set_open.set(false);
        set_highlight.set(0);
    };

    let open_list = move || {
        if disabled.get_untracked() {
            return;
        }
        set_open.set(true);
        set_typing.set(true);
        set_query.set(String::new());
        set_highlight.set(0);
    };

    let close_list = move || {
        set_open.set(false);
        set_typing.set(false);
        set_query.set(String::new());
    };

    let placeholder_text = placeholder.unwrap_or_else(|| all_label.clone());
    let list_id = format!(
        "filter-combobox-{}",
        label
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            })
            .collect::<String>()
    );

    view! {
        <div class=COMBOBOX data-open=move || open.get().to_string()>
            <label class=COMBOBOX_LABEL>
                <span>{label.clone()}</span>
                <div class=COMBOBOX_CONTROL>
                    <input
                        type="text"
                        class=COMBOBOX_INPUT
                        role="combobox"
                        aria-autocomplete="list"
                        aria-expanded=move || open.get().to_string()
                        aria-controls=list_id.clone()
                        aria-haspopup="listbox"
                        prop:value=move || display_value.get()
                        prop:placeholder=placeholder_text.clone()
                        prop:disabled=move || disabled.get()
                        on:focus=move |_| open_list()
                        on:click=move |_| {
                            if !open.get_untracked() {
                                open_list();
                            }
                        }
                        on:blur=move |_| {
                            // Defer so option mousedown can commit first.
                            #[cfg(feature = "hydrate")]
                            {
                                use wasm_bindgen::JsCast;
                                use wasm_bindgen::closure::Closure;
                                if let Some(window) = web_sys::window() {
                                    let close = close_list;
                                    let cb = Closure::once_into_js(move || {
                                        close();
                                    });
                                    let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                                        cb.as_ref().unchecked_ref(),
                                        120,
                                    );
                                } else {
                                    close_list();
                                }
                            }
                            #[cfg(not(feature = "hydrate"))]
                            {
                                close_list();
                            }
                        }
                        on:input=move |event| {
                            if disabled.get_untracked() {
                                return;
                            }
                            let next = event_target_value(&event);
                            set_typing.set(true);
                            set_query.set(next);
                            set_open.set(true);
                            set_highlight.set(0);
                        }
                        on:keydown=move |event| {
                            if disabled.get_untracked() {
                                return;
                            }
                            #[cfg(feature = "hydrate")]
                            {
                                let event: KeyboardEvent = event;
                                let key = event.key();
                                let items = filtered.get_untracked();
                                if items.is_empty() {
                                    return;
                                }
                                match key.as_str() {
                                    "ArrowDown" => {
                                        event.prevent_default();
                                        if !open.get_untracked() {
                                            open_list();
                                        }
                                        let len = items.len();
                                        set_highlight.update(|i| *i = (*i + 1) % len);
                                    }
                                    "ArrowUp" => {
                                        event.prevent_default();
                                        if !open.get_untracked() {
                                            open_list();
                                        }
                                        let len = items.len();
                                        set_highlight.update(|i| {
                                            *i = if *i == 0 { len - 1 } else { *i - 1 };
                                        });
                                    }
                                    "Enter" => {
                                        if open.get_untracked() {
                                            event.prevent_default();
                                            let idx =
                                                highlight.get_untracked().min(items.len() - 1);
                                            if let Some(opt) = items.get(idx) {
                                                commit(opt.value.clone());
                                            }
                                        }
                                    }
                                    "Tab" => {
                                        if open.get_untracked() {
                                            let idx =
                                                highlight.get_untracked().min(items.len() - 1);
                                            if let Some(opt) = items.get(idx) {
                                                commit(opt.value.clone());
                                            }
                                        }
                                    }
                                    "Escape" => {
                                        event.prevent_default();
                                        close_list();
                                    }
                                    _ => {}
                                }
                            }
                            #[cfg(not(feature = "hydrate"))]
                            {
                                let _ = event;
                            }
                        }
                    />
                    <span class=COMBOBOX_CHEVRON aria-hidden="true"></span>
                </div>
            </label>
            <Show when=move || open.get()>
                <ul class=COMBOBOX_LIST id=list_id.clone() role="listbox">
                    {move || {
                        let items = filtered.get();
                        if items.is_empty() {
                            return view! {
                                <li class=COMBOBOX_EMPTY role="presentation">
                                    "No matches"
                                </li>
                            }
                            .into_any();
                        }
                        let hi = highlight.get();
                        items
                            .into_iter()
                            .enumerate()
                            .map(|(idx, opt)| {
                                let selected = value.get() == opt.value;
                                let active = idx == hi;
                                let opt_value = opt.value.clone();
                                let opt_label = opt.label.clone();
                                let option_class = format!(
                                    "{COMBOBOX_OPTION}{}{}",
                                    if active {
                                        format!(" {COMBOBOX_OPTION_ACTIVE}")
                                    } else {
                                        String::new()
                                    },
                                    if selected {
                                        format!(" {COMBOBOX_OPTION_SELECTED}")
                                    } else {
                                        String::new()
                                    },
                                );
                                view! {
                                    <li
                                        class=option_class
                                        role="option"
                                        aria-selected=selected.to_string()
                                        on:mousedown=move |event| {
                                            event.prevent_default();
                                            commit(opt_value.clone());
                                        }
                                        on:mouseenter=move |_| set_highlight.set(idx)
                                    >
                                        {opt_label}
                                    </li>
                                }
                            })
                            .collect_view()
                            .into_any()
                    }}
                </ul>
            </Show>
        </div>
    }
}
