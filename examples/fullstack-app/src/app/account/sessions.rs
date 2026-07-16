#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use crate::app::helpers::{redirect_browser, server_error_text};
use crate::app::{
    ListAccountSessions, RevokeAccountSession, browser_load, list_account_sessions,
    revoke_account_session,
};
use crate::contracts::AccountSessionSummary;
use crate::ui::account_page_shell;
use crate::ui::classes::{
    ACCOUNT_LEDE_FLUSH, ACCOUNT_PANEL, ACCOUNT_PANEL_HEAD, ACCOUNT_PANEL_TITLE, BANNER_ERROR,
    BANNER_SUCCESS, BTN_PRIMARY, BTN_SECONDARY, CLIENT_DATA_SLOT, PANEL_COMPACT, RESULT_LINE,
    SECTION_LABEL, SESSION_ASSURANCE, SESSION_CARD, SESSION_CARD_CURRENT, SESSION_CARD_HEAD,
    SESSION_LIST, with_extra,
};
use leptos::prelude::*;
use server_fn::ServerFnError;
#[cfg(feature = "hydrate")]
use web_sys::window;

#[component]
pub fn AccountSessionsPage() -> impl IntoView {
    account_page_shell(
        "Sessions",
        "Review and revoke browser access for this account.",
        "sessions",
        view! { <AccountSessionManager /> },
    )
}

#[island(lazy)]
pub fn AccountSessionManager() -> impl IntoView {
    let sessions = browser_load(list_account_sessions);
    let revoke_action = ServerAction::<RevokeAccountSession>::new();
    let revoke_pending = revoke_action.pending();
    let revoke_value = revoke_action.value();
    let (rows, set_rows) = signal(Vec::<AccountSessionSummary>::new());
    let (pending_id, set_pending_id) = signal(None::<String>);
    let (pending_is_current, set_pending_is_current) = signal(false);
    let (status_message, set_status_message) = signal(None::<String>);
    let (error_message, set_error_message) = signal(None::<String>);
    let (signing_out, set_signing_out) = signal(false);

    Effect::new(move |_| {
        if let Some(Ok(response)) = sessions.get() {
            set_rows.set(response.sessions);
        }
    });

    Effect::new(move |_| match revoke_value.get() {
        Some(Ok(_)) => {
            let id = pending_id.get_untracked();
            let was_current = pending_is_current.get_untracked();
            set_pending_id.set(None);
            set_error_message.set(None);
            if was_current {
                // Self-revoke: cookie cleared server-side — leave immediately (hard nav).
                set_signing_out.set(true);
                set_status_message.set(Some("Signing you out…".to_owned()));
                redirect_browser("/login");
                #[cfg(feature = "hydrate")]
                if let Some(window) = window() {
                    let _ = window.location().set_href("/login");
                }
                return;
            }
            if let Some(id) = id {
                set_rows.update(|list| list.retain(|session| session.session_id != id));
            }
            set_status_message.set(Some(
                "Session revoked. That device is signed out immediately if online, or on its next request if offline."
                    .to_owned(),
            ));
        }
        Some(Err(error)) => {
            set_pending_id.set(None);
            set_pending_is_current.set(false);
            set_signing_out.set(false);
            set_status_message.set(None);
            set_error_message.set(Some(server_error_text(error)));
        }
        None => {}
    });

    view! {
        <section class=ACCOUNT_PANEL>
            <div class=ACCOUNT_PANEL_HEAD>
                <div>
                    <p class=SECTION_LABEL>"Devices"</p>
                    <h2 class=ACCOUNT_PANEL_TITLE>"Active sessions"</h2>
                </div>
            </div>
            <p class=ACCOUNT_LEDE_FLUSH>
                "Revoking ends access for that browser or device. Signing out this browser leaves the page immediately."
            </p>
            <div class=CLIENT_DATA_SLOT>
                {move || match sessions.get() {
                    Some(Ok(_)) => {
                        let list = rows.get();
                        if list.is_empty() {
                            view! { <p class=RESULT_LINE>"No active sessions"</p> }.into_any()
                        } else {
                            view! {
                                <div class=SESSION_LIST>
                                    <For
                                        each=move || rows.get()
                                        key=|session| session.session_id.clone()
                                        children=move |session| {
                                            let session_id = session.session_id.clone();
                                            let session_id_disabled = session_id.clone();
                                            let session_id_click = session_id.clone();
                                            let session_id_label = session_id.clone();
                                            let is_current = session.current;
                                            let assurance = session.assurance.clone();
                                            let expires = session.expires_at_ms;
                                            view! {
                                                <article class=if is_current {
                                                    with_extra(PANEL_COMPACT, Some(SESSION_CARD_CURRENT))
                                                } else {
                                                    with_extra(PANEL_COMPACT, Some(SESSION_CARD))
                                                }>
                                                    <div class=SESSION_CARD_HEAD>
                                                        <h3 class="m-0 text-sm font-semibold">{if is_current { "This browser" } else { "Other device" }}</h3>
                                                        <span class=SESSION_ASSURANCE>{assurance.to_uppercase()}</span>
                                                    </div>
                                                    <p class=RESULT_LINE>
                                                        {format!("Expires at {expires}")}
                                                    </p>
                                                    <button
                                                        type="button"
                                                        class=if is_current { BTN_PRIMARY } else { BTN_SECONDARY }
                                                        disabled=move || {
                                                            revoke_pending.get()
                                                                || signing_out.get()
                                                                || pending_id.get().as_deref()
                                                                    == Some(session_id_disabled.as_str())
                                                        }
                                                        on:click=move |_| {
                                                            set_error_message.set(None);
                                                            set_status_message.set(None);
                                                            set_pending_id.set(Some(session_id_click.clone()));
                                                            set_pending_is_current.set(is_current);
                                                            if is_current {
                                                                set_signing_out.set(true);
                                                                set_status_message.set(Some(
                                                                    "Signing you out of this browser…".to_owned(),
                                                                ));
                                                            }
                                                            revoke_action.dispatch(RevokeAccountSession {
                                                                session_id: session_id_click.clone(),
                                                            });
                                                        }
                                                    >
                                                        {move || {
                                                            let this_pending = pending_id.get().as_deref()
                                                                == Some(session_id_label.as_str())
                                                                && (revoke_pending.get() || signing_out.get());
                                                            if this_pending {
                                                                if is_current { "Signing out…" } else { "Revoking…" }
                                                            } else if is_current {
                                                                "Sign out this browser"
                                                            } else {
                                                                "Revoke access"
                                                            }
                                                        }}
                                                    </button>
                                                </article>
                                            }
                                        }
                                    />
                                </div>
                            }.into_any()
                        }
                    }
                    Some(Err(error)) => view! { <p class=BANNER_ERROR>{server_error_text(error)}</p> }.into_any(),
                    None => view! { <p class=RESULT_LINE>"Loading sessions"</p> }.into_any(),
                }}
            </div>
            <p class=BANNER_SUCCESS hidden=move || status_message.get().is_none() || error_message.get().is_some()>
                {move || status_message.get().unwrap_or_default()}
            </p>
            <p class=BANNER_ERROR hidden=move || error_message.get().is_none()>
                {move || error_message.get().unwrap_or_default()}
            </p>
        </section>
    }
}
