use leptos::prelude::*;

use super::classes::{PANEL, PANEL_COMPACT, with_extra};

/// Standard surface card (Tailwind utilities; legacy `.panel`).
#[component]
pub fn Panel(#[prop(optional, into)] class: Option<String>, children: Children) -> impl IntoView {
    let class_name = with_extra(PANEL, class.as_deref());
    view! {
        <section class=class_name>
            {children()}
        </section>
    }
}

/// Nested / compact card (Tailwind utilities; legacy `.compact-panel`).
#[component]
pub fn CompactPanel(
    #[prop(optional, into)] class: Option<String>,
    children: Children,
) -> impl IntoView {
    let class_name = with_extra(PANEL_COMPACT, class.as_deref());
    view! {
        <article class=class_name>
            {children()}
        </article>
    }
}
