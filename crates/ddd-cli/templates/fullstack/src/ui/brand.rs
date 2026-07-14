use leptos::prelude::*;

/// Auth / error card brand mark (matches existing `.auth-brand` CSS).
#[component]
pub fn AuthBrand() -> impl IntoView {
    view! {
        <div class="auth-brand">
            <span class="auth-logo" aria-hidden="true">"d"</span>
            <div>
                <p class="auth-brand-name">"wasi-auth"</p>
                <p class="auth-brand-meta">"Secure workspace access"</p>
            </div>
        </div>
    }
}
