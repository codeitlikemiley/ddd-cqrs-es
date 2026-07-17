use leptos::prelude::*;

use super::classes::{BANNER_ERROR, BANNER_SUCCESS, RESULT_LINE};

/// Error banner. Hidden when `message` is empty.
#[component]
pub fn ErrorBanner(#[prop(into)] message: Signal<Option<String>>) -> impl IntoView {
    view! {
        <p class=BANNER_ERROR hidden=move || message.get().as_ref().is_none_or(|m| m.is_empty())>
            {move || message.get().unwrap_or_default()}
        </p>
    }
}

/// Success / notice banner.
#[component]
pub fn SuccessBanner(#[prop(into)] message: Signal<Option<String>>) -> impl IntoView {
    view! {
        <p class=BANNER_SUCCESS hidden=move || message.get().as_ref().is_none_or(|m| m.is_empty())>
            <span>{move || message.get().unwrap_or_default()}</span>
        </p>
    }
}

/// Muted result / status line.
#[component]
pub fn ResultLine(children: Children) -> impl IntoView {
    view! {
        <p class=RESULT_LINE>{children()}</p>
    }
}
