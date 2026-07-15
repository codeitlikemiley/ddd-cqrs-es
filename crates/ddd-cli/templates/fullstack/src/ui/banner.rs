use leptos::prelude::*;

/// Error banner (`.error-banner`). Hidden when `message` is empty.
#[component]
pub fn ErrorBanner(#[prop(into)] message: Signal<Option<String>>) -> impl IntoView {
    view! {
        <p class="error-banner" hidden=move || message.get().as_ref().is_none_or(|m| m.is_empty())>
            {move || message.get().unwrap_or_default()}
        </p>
    }
}

/// Success banner (`.auth-success`).
#[component]
pub fn SuccessBanner(#[prop(into)] message: Signal<Option<String>>) -> impl IntoView {
    view! {
        <p class="auth-success" hidden=move || message.get().as_ref().is_none_or(|m| m.is_empty())>
            <span>{move || message.get().unwrap_or_default()}</span>
        </p>
    }
}

/// Muted result / status line (`.result-line`).
#[component]
pub fn ResultLine(children: Children) -> impl IntoView {
    view! {
        <p class="result-line">{children()}</p>
    }
}
