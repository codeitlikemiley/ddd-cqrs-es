use leptos::prelude::*;

use super::classes::{AUTH_BRAND, AUTH_BRAND_META, AUTH_BRAND_NAME, AUTH_LOGO};

/// Auth / error card brand mark.
#[component]
pub fn AuthBrand() -> impl IntoView {
    view! {
        <div class=AUTH_BRAND>
            <span class=AUTH_LOGO aria-hidden="true">"d"</span>
            <div>
                <p class=AUTH_BRAND_NAME>"wasi-auth"</p>
                <p class=AUTH_BRAND_META>"Secure workspace access"</p>
            </div>
        </div>
    }
}
