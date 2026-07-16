//! Legacy navigation pills for organization management pages.

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use leptos::prelude::*;

#[component]
pub fn OrganizationLinks() -> impl IntoView {
    view! {
        <section class="panel panel-inline">
            <a class="text-link" href="/organizations">"Back to organizations"</a>
        </section>
    }
}
