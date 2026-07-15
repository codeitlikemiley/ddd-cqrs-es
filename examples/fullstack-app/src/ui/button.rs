use leptos::ev::MouseEvent;
use leptos::prelude::*;

/// Primary action button (existing `.primary-button` styles).
#[component]
pub fn PrimaryButton(
    /// Extra classes appended after `primary-button` (e.g. modifiers).
    #[prop(optional, into)]
    class: Option<String>,
    #[prop(optional, into)] disabled: Signal<bool>,
    #[prop(optional)] button_type: Option<&'static str>,
    #[prop(optional)] on_click: Option<Callback<MouseEvent>>,
    children: Children,
) -> impl IntoView {
    let class_name = move || match class.clone() {
        Some(extra) if !extra.trim().is_empty() => format!("primary-button {extra}"),
        _ => "primary-button".to_owned(),
    };
    let ty = button_type.unwrap_or("button");
    view! {
        <button
            type=ty
            class=class_name
            disabled=move || disabled.get()
            on:click=move |ev| {
                if let Some(cb) = on_click {
                    cb.run(ev);
                }
            }
        >
            {children()}
        </button>
    }
}

/// Secondary action button (existing `.secondary-button` styles).
#[component]
pub fn SecondaryButton(
    #[prop(optional, into)] class: Option<String>,
    #[prop(optional, into)] disabled: Signal<bool>,
    #[prop(optional)] button_type: Option<&'static str>,
    #[prop(optional)] on_click: Option<Callback<MouseEvent>>,
    children: Children,
) -> impl IntoView {
    let class_name = move || match class.clone() {
        Some(extra) if !extra.trim().is_empty() => format!("secondary-button {extra}"),
        _ => "secondary-button".to_owned(),
    };
    let ty = button_type.unwrap_or("button");
    view! {
        <button
            type=ty
            class=class_name
            disabled=move || disabled.get()
            on:click=move |ev| {
                if let Some(cb) = on_click {
                    cb.run(ev);
                }
            }
        >
            {children()}
        </button>
    }
}

/// Text-style control rendered as an anchor (existing `.link-button`).
#[component]
pub fn LinkButton(
    href: &'static str,
    #[prop(optional, into)] class: Option<String>,
    children: Children,
) -> impl IntoView {
    let class_name = match class {
        Some(extra) if !extra.trim().is_empty() => format!("link-button {extra}"),
        _ => "link-button".to_owned(),
    };
    view! {
        <a class=class_name href=href>
            {children()}
        </a>
    }
}
