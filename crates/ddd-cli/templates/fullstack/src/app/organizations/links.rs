//! Legacy navigation pills for organization management pages.

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use crate::ui::classes::{PANEL_INLINE, TEXT_LINK};
use leptos::prelude::*;

#[component]
pub fn OrganizationLinks() -> impl IntoView {
    view! {
        <section class=PANEL_INLINE>
            <a class=TEXT_LINK href="/organizations">"Back to organizations"</a>
        </section>
    }
}
