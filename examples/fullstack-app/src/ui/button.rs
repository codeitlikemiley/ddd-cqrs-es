use leptos::ev::MouseEvent;
use leptos::prelude::*;

use super::classes::{BTN_PRIMARY, BTN_SECONDARY, with_extra};

/// Primary action button (Tailwind utilities; legacy `.primary-button`).
#[component]
pub fn PrimaryButton(
    /// Extra classes appended after the primary button utilities.
    #[prop(optional, into)]
    class: Option<String>,
    #[prop(optional, into)] disabled: Signal<bool>,
    #[prop(optional)] button_type: Option<&'static str>,
    #[prop(optional)] on_click: Option<Callback<MouseEvent>>,
    children: Children,
) -> impl IntoView {
    let class_name = move || with_extra(BTN_PRIMARY, class.as_deref());
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

/// Secondary action button (Tailwind utilities; legacy `.secondary-button`).
#[component]
pub fn SecondaryButton(
    #[prop(optional, into)] class: Option<String>,
    #[prop(optional, into)] disabled: Signal<bool>,
    #[prop(optional)] button_type: Option<&'static str>,
    #[prop(optional)] on_click: Option<Callback<MouseEvent>>,
    children: Children,
) -> impl IntoView {
    let class_name = move || with_extra(BTN_SECONDARY, class.as_deref());
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

/// Text-style control rendered as an anchor (secondary chrome).
#[component]
pub fn LinkButton(
    href: &'static str,
    #[prop(optional, into)] class: Option<String>,
    children: Children,
) -> impl IntoView {
    let class_name = with_extra(BTN_SECONDARY, class.as_deref());
    view! {
        <a class=class_name href=href>
            {children()}
        </a>
    }
}
