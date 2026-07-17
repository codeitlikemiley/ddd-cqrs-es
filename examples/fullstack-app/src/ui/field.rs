use leptos::prelude::*;

use super::classes::{FIELD, FIELD_GROUP, FIELD_HINT, INPUT, with_extra};

/// Vertical stack of fields (legacy `.auth-fields`).
#[component]
pub fn FieldGroup(children: Children) -> impl IntoView {
    view! {
        <div class=FIELD_GROUP>
            {children()}
        </div>
    }
}

/// Labeled field shell (legacy `.auth-field`). Label text + control + optional hint.
#[component]
pub fn Field(
    label: &'static str,
    #[prop(optional)] hint: Option<&'static str>,
    children: Children,
) -> impl IntoView {
    view! {
        <label class=FIELD>
            <span>{label}</span>
            {children()}
            {hint.map(|text| view! { <small class=FIELD_HINT>{text}</small> })}
        </label>
    }
}

/// Text input with shared input chrome (legacy `.auth-input`).
#[component]
pub fn TextInput(
    #[prop(optional)] input_type: Option<&'static str>,
    #[prop(optional)] autocomplete: Option<&'static str>,
    #[prop(optional)] maxlength: Option<u32>,
    #[prop(optional)] placeholder: Option<&'static str>,
    #[prop(optional, into)] value: Signal<String>,
    #[prop(optional)] class: Option<&'static str>,
    /// Called with the input element's string value on each input event.
    #[prop(into)]
    on_input: Callback<String>,
) -> impl IntoView {
    let ty = input_type.unwrap_or("text");
    let class_name = with_extra(INPUT, class);
    view! {
        <input
            class=class_name
            type=ty
            autocomplete=autocomplete.unwrap_or("")
            maxlength=maxlength.map(|n| n.to_string()).unwrap_or_default()
            placeholder=placeholder.unwrap_or("")
            prop:value=move || value.get()
            on:input=move |event| {
                on_input.run(event_target_value(&event));
            }
        />
    }
}
