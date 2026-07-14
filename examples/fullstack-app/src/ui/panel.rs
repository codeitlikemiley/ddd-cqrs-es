use leptos::prelude::*;

/// Standard surface card (`.panel`).
#[component]
pub fn Panel(
    #[prop(optional, into)]
    class: Option<String>,
    children: Children,
) -> impl IntoView {
    let class_name = match class {
        Some(extra) if !extra.trim().is_empty() => format!("panel {extra}"),
        _ => "panel".to_owned(),
    };
    view! {
        <section class=class_name>
            {children()}
        </section>
    }
}

/// Nested / compact card (`.compact-panel`).
#[component]
pub fn CompactPanel(
    #[prop(optional, into)]
    class: Option<String>,
    children: Children,
) -> impl IntoView {
    let class_name = match class {
        Some(extra) if !extra.trim().is_empty() => format!("compact-panel {extra}"),
        _ => "compact-panel".to_owned(),
    };
    view! {
        <article class=class_name>
            {children()}
        </article>
    }
}
