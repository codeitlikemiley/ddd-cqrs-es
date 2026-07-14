use leptos::prelude::*;

/// Uppercase section label (`.section-label`).
#[component]
pub fn SectionLabel(children: Children) -> impl IntoView {
    view! {
        <p class="section-label">{children()}</p>
    }
}
