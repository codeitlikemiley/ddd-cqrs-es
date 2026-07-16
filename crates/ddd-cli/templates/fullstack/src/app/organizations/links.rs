//! Legacy navigation pills for organization management pages.

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use crate::ui::classes::{
    AUTH_TEXT_LINK, BANNER_ERROR, BANNER_SUCCESS, BTN_AUTH_SUBMIT, BTN_PRIMARY, BTN_SECONDARY,
    BUTTON_ROW, FIELD, FIELD_GROUP, INPUT, PANEL, PANEL_COMPACT, RESULT_LINE, SECTION_LABEL,
};
use leptos::prelude::*;

#[component]
pub fn OrganizationLinks() -> impl IntoView {
    view! {
        <section class=format!("{}{}", PANEL, " panel-inline")>
            <a class="text-link" href="/organizations">"Back to organizations"</a>
        </section>
    }
}
