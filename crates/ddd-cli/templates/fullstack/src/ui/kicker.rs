use leptos::prelude::*;

use super::classes::SECTION_LABEL;

/// Uppercase section kicker label.
#[component]
pub fn SectionLabel(children: Children) -> impl IntoView {
    view! {
        <p class=SECTION_LABEL>{children()}</p>
    }
}
