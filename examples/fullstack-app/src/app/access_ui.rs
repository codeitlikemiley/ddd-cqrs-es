//! Leptos helpers for capability-gated UI (presentation only).

#![allow(unused_imports)]

use crate::access::{AccessContext, AccessRequirement};
use crate::app::{browser_load, get_current_session};
use leptos::prelude::*;

/// Load session and map to [`AccessContext`] (None while loading / error).
#[must_use]
pub fn access_context_from_session_resource(
    session: ReadSignal<
        Option<Result<crate::contracts::SessionView, server_fn::ServerFnError>>,
    >,
) -> Memo<AccessContext> {
    Memo::new(move |_| match session.get() {
        Some(Ok(view)) if view.authenticated => AccessContext::from_session(&view),
        Some(Ok(_)) => AccessContext::anonymous(),
        Some(Err(_)) | None => AccessContext::anonymous(),
    })
}

/// Hide children unless `requirement` is satisfied by the current session.
///
/// Presentation only — always enforce the same requirement on the server.
#[component]
pub fn CanAccess(requirement: AccessRequirement, children: ChildrenFn) -> impl IntoView {
    let session = browser_load(get_current_session);
    let ctx = Memo::new(move |_| match session.get() {
        Some(Ok(view)) if view.authenticated => AccessContext::from_session(&view),
        _ => AccessContext::anonymous(),
    });
    let allowed = Memo::new({
        let requirement = requirement.clone();
        move |_| requirement.is_satisfied_by(&ctx.get())
    });
    view! {
        {move || {
            if allowed.get() {
                children().into_any()
            } else {
                ().into_any()
            }
        }}
    }
}
