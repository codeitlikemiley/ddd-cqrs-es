#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use crate::app::helpers::{
    action_result_text, is_passkey_cancel_message, redirect_browser, server_error_text,
};
#[cfg(feature = "hydrate")]
use crate::app::helpers::{passkey_js_error, passkey_js_string};
use crate::app::{
    GetAuthCapabilities, StartPasskeyRegistration, VerifyPasskeyRegistration, browser_load,
    get_auth_capabilities, get_current_session, start_passkey_registration,
    verify_passkey_registration,
};
#[cfg(feature = "hydrate")]
use crate::app::{create_passkey_credential, passkey_supported};
use crate::contracts::{AuthCapabilities, SessionView};
use crate::ui::account_page_shell;
use crate::ui::classes::{
    ACCOUNT_LEDE, ACCOUNT_PANEL, ACCOUNT_PANEL_TITLE, BADGE, BADGE_OFF, BADGE_ON, BANNER_ERROR,
    BTN_PRIMARY, BTN_SECONDARY, BUTTON_ROW, KV_DD, KV_DT, MFA_STATUS_KV, PASSKEY_BUTTON_MT,
    PASSKEY_DEVICE_BAR, PASSKEY_DEVICE_BAR_SHORT, PASSKEY_DEVICE_CARD, PASSKEY_DEVICE_CARD_P,
    PASSKEY_DEVICE_ICON, PASSKEY_FLOW, PASSKEY_FOCUS_PANEL, PASSKEY_FOCUS_WRAP, PASSKEY_HINT,
    PASSKEY_OVERVIEW, PASSKEY_ROW_MT, RESULT_LINE, SECTION_LABEL, STATUS_HEAD, STEPS_PREVIEW,
    STEPS_PREVIEW_STRONG, WIZARD_LINE, WIZARD_LINE_DONE, WIZARD_PROGRESS, WIZARD_STEP,
    WIZARD_STEP_ACTIVE, with_extra,
};
use leptos::prelude::*;
#[cfg(feature = "hydrate")]
use leptos::task::spawn_local;
use server_fn::ServerFnError;

#[component]
pub fn AccountPasskeysPage() -> impl IntoView {
    account_page_shell(
        "Passkeys",
        "Sign in with Face ID, Touch ID, Windows Hello, or a security key — no password to type.",
        "passkeys",
        view! { <PasskeyManager /> },
    )
}

/// Account passkeys (GitHub / Google / Apple-style):
/// status → create ceremony focus → success. Never renders blank.
#[island(lazy)]
pub fn PasskeyManager() -> impl IntoView {
    let capabilities = browser_load(get_auth_capabilities);
    let session = browser_load(get_current_session);
    let start_action = ServerAction::<StartPasskeyRegistration>::new();
    let verify_action = ServerAction::<VerifyPasskeyRegistration>::new();
    let start_pending = start_action.pending();
    let verify_pending = verify_action.pending();
    let start_value = start_action.value();
    let verify_value = verify_action.value();
    let (client_error, set_client_error) = signal(None::<String>);
    #[cfg(feature = "hydrate")]
    let (browser_ok, set_browser_ok) = signal(passkey_supported());
    #[cfg(not(feature = "hydrate"))]
    let (browser_ok, _) = signal(true);

    Effect::new(move |_| {
        if let Some(Ok(response)) = start_value.get() {
            #[cfg(feature = "hydrate")]
            {
                if !passkey_supported() {
                    set_browser_ok.set(false);
                    set_client_error.set(Some(
                        "This browser or device does not support passkeys.".to_owned(),
                    ));
                    return;
                }
                let verify_action = verify_action;
                let set_client_error = set_client_error;
                let challenge_id = response.challenge_id;
                let options_json = response.public_key_options_json;
                set_client_error.set(None);
                spawn_local(async move {
                    match create_passkey_credential(options_json).await {
                        Ok(value) => match passkey_js_string(value) {
                            Ok(credential_json) => {
                                verify_action.dispatch(VerifyPasskeyRegistration {
                                    challenge_id,
                                    credential_json,
                                    redirect_url: Some("/account/passkeys".to_owned()),
                                });
                            }
                            Err(error) => set_client_error.set(Some(error)),
                        },
                        Err(error) => set_client_error.set(Some(passkey_js_error(error))),
                    }
                });
            }
            #[cfg(not(feature = "hydrate"))]
            {
                let _ = response;
            }
        }
    });

    view! {
        <div class=PASSKEY_FLOW>
            {move || {
                // Stay on ceremony surface for prompt, save, OR error (do not flash back).
                let ceremony_active = start_pending.get()
                    || verify_pending.get()
                    || matches!(start_value.get(), Some(Ok(_)))
                    || client_error.get().is_some()
                    || matches!(start_value.get(), Some(Err(_)))
                    || matches!(verify_value.get(), Some(Err(_)));
                let registered_ok = matches!(verify_value.get(), Some(Ok(_)));
                let ceremony_failed = client_error.get().is_some()
                    || matches!(start_value.get(), Some(Err(_)))
                    || matches!(verify_value.get(), Some(Err(_)));
                let ceremony_cancelled = client_error
                    .get()
                    .as_ref()
                    .is_some_and(|message| is_passkey_cancel_message(message));

                // Exclusive focus while OS/browser passkey sheet is active
                if ceremony_active && !registered_ok {
                    return view! {
                        <div class=PASSKEY_FOCUS_WRAP>
                            <section class=PASSKEY_FOCUS_PANEL>
                                <div class=WIZARD_PROGRESS aria-hidden="true">
                                    <span class=WIZARD_STEP_ACTIVE>"1"</span>
                                    <span class=WIZARD_LINE_DONE></span>
                                    <span class=WIZARD_STEP_ACTIVE>"2"</span>
                                    <span class=WIZARD_LINE></span>
                                    <span class=WIZARD_STEP>"3"</span>
                                </div>
                                <p class=SECTION_LABEL>"Creating passkey"</p>
                                <h2 class=ACCOUNT_PANEL_TITLE>
                                    {move || if ceremony_cancelled {
                                        "Passkey not created"
                                    } else if ceremony_failed {
                                        "Could not create passkey"
                                    } else {
                                        "Confirm with your device"
                                    }}
                                </h2>
                                <p class=ACCOUNT_LEDE>
                                    {move || if ceremony_cancelled {
                                        "You closed the browser prompt. Try again when you're ready, or use a different device / security key."
                                    } else if ceremony_failed {
                                        "You can retry the browser prompt, or go back and try another device / security key."
                                    } else {
                                        "Use Face ID, Touch ID, Windows Hello, a phone QR passkey, or a security key. Keep this tab open until the prompt finishes."
                                    }}
                                </p>
                                <div class=PASSKEY_DEVICE_CARD aria-hidden="true" hidden=move || ceremony_failed>
                                    <div class=PASSKEY_DEVICE_ICON>
                                        <span class=PASSKEY_DEVICE_BAR></span>
                                        <span class=PASSKEY_DEVICE_BAR_SHORT></span>
                                    </div>
                                    <p class=PASSKEY_DEVICE_CARD_P>"Waiting for authenticator…"</p>
                                </div>
                                <p class=RESULT_LINE hidden=move || ceremony_failed>
                                    {move || if verify_pending.get() {
                                        "Saving passkey to your account…"
                                    } else if start_pending.get() {
                                        "Starting secure registration…"
                                    } else {
                                        "Follow the prompt on your device"
                                    }}
                                </p>
                                <p
                                    class=BANNER_ERROR
                                    hidden=move || {
                                        match client_error.get() {
                                            None => true,
                                            // Cancel is explained by the lede; avoid a red stack-style banner.
                                            Some(message) if is_passkey_cancel_message(&message) => {
                                                true
                                            }
                                            Some(_) => false,
                                        }
                                    }
                                >
                                    {move || client_error.get().unwrap_or_default()}
                                </p>
                                <p class=BANNER_ERROR hidden=move || !matches!(start_value.get(), Some(Err(_)))>
                                    {move || match start_value.get() {
                                        Some(Err(error)) => server_error_text(error),
                                        _ => String::new(),
                                    }}
                                </p>
                                <p class=BANNER_ERROR hidden=move || !matches!(verify_value.get(), Some(Err(_)))>
                                    {move || match verify_value.get() {
                                        Some(Err(error)) => server_error_text(error),
                                        _ => String::new(),
                                    }}
                                </p>
                                <div class=with_extra(BUTTON_ROW, Some(PASSKEY_ROW_MT))>
                                    <button
                                        type="button"
                                        class=BTN_PRIMARY
                                        disabled=move || start_pending.get() || verify_pending.get()
                                        on:click=move |_| {
                                            set_client_error.set(None);
                                            let email = session
                                                .get_untracked()
                                                .and_then(Result::ok)
                                                .and_then(|s| s.primary_email);
                                            start_action.dispatch(StartPasskeyRegistration {
                                                email,
                                                redirect_url: Some("/account/passkeys".to_owned()),
                                            });
                                        }
                                    >"Try again"</button>
                                    <button
                                        type="button"
                                        class=BTN_SECONDARY
                                        on:click=move |_| {
                                            set_client_error.set(None);
                                            redirect_browser("/account/passkeys");
                                        }
                                    >"Back"</button>
                                </div>
                            </section>
                        </div>
                    }.into_any();
                }

                if registered_ok {
                    return view! {
                        <div class=PASSKEY_FOCUS_WRAP>
                            <section class=PASSKEY_FOCUS_PANEL>
                                <span class=format!("{} {}", BADGE, BADGE_ON)>"Passkey registered"</span>
                                <h2 class=ACCOUNT_PANEL_TITLE>"You can sign in without a password"</h2>
                                <p class=ACCOUNT_LEDE>
                                    "Next time, choose passkey on the sign-in page and approve with this device. Your session assurance is elevated for phishing-resistant sign-in."
                                </p>
                                <div class=with_extra(BUTTON_ROW, Some(PASSKEY_ROW_MT))>
                                    <a class=BTN_PRIMARY href="/account/sessions">"Review sessions"</a>
                                    <a class=BTN_SECONDARY href="/account/profile">"Back to profile"</a>
                                </div>
                            </section>
                        </div>
                    }.into_any();
                }

                // Default overview (always visible — never blank)
                let session_email = session
                    .get()
                    .and_then(Result::ok)
                    .and_then(|s| s.primary_email)
                    .unwrap_or_else(|| "your account".to_owned());
                let email_for_register = session
                    .get()
                    .and_then(Result::ok)
                    .and_then(|s| s.primary_email);
                let can_register = email_for_register.is_some();
                let email_dispatch = email_for_register.clone();
                let caps = capabilities.get();
                let passkeys_on = caps
                    .as_ref()
                    .and_then(|r| r.as_ref().ok())
                    .is_some_and(|c| c.passkeys_enabled);
                let caps_loaded = caps.is_some();
                let caps_error = matches!(caps, Some(Err(_)));
                let device_ok = browser_ok.get();

                let badge = if !caps_loaded {
                    "Loading"
                } else if passkeys_on && device_ok {
                    "Available"
                } else {
                    "Not ready"
                };
                let badge_on = passkeys_on && device_ok;
                let deployment_label = if !caps_loaded {
                    "Checking…"
                } else if caps_error {
                    "Error"
                } else if passkeys_on {
                    "Enabled"
                } else {
                    "Disabled"
                };

                view! {
                    <div class=PASSKEY_OVERVIEW>
                        <section class=ACCOUNT_PANEL>
                            <div class=STATUS_HEAD>
                                <div>
                                    <p class=SECTION_LABEL>"Phishing-resistant sign-in"</p>
                                    <h2 class=ACCOUNT_PANEL_TITLE>"Passkeys"</h2>
                                    <p class=ACCOUNT_LEDE>
                                        "A passkey lets you sign in with the biometrics or PIN already on this device. It cannot be phished like a password."
                                    </p>
                                </div>
                                {if badge_on {
                                    view! { <span class=format!("{} {}", BADGE, BADGE_ON)>{badge}</span> }.into_any()
                                } else {
                                    view! { <span class=format!("{} {}", BADGE, BADGE_OFF)>{badge}</span> }.into_any()
                                }}
                            </div>
                            <dl class=MFA_STATUS_KV>
                                <dt class=KV_DT>"Account"</dt>
                                <dd class=KV_DD>{session_email.clone()}</dd>
                                <dt class=KV_DT>"Deployment"</dt>
                                <dd class=KV_DD>{deployment_label}</dd>
                                <dt class=KV_DT>"This browser"</dt>
                                <dd class=KV_DD>{if device_ok { "Supports WebAuthn" } else { "No passkey API" }}</dd>
                            </dl>
                        </section>

                        {if !caps_loaded {
                            view! {
                                <section class=ACCOUNT_PANEL>
                                    <p class=RESULT_LINE>"Loading passkey settings…"</p>
                                </section>
                            }.into_any()
                        } else if !passkeys_on {
                            view! {
                                <section class=ACCOUNT_PANEL>
                                    <p class=SECTION_LABEL>"Operator note"</p>
                                    <h2 class=ACCOUNT_PANEL_TITLE>"Passkeys are off for this deployment"</h2>
                                    <p class=ACCOUNT_LEDE>
                                        "Turn them on with AUTH_ENABLE_PASSKEYS=true, then set AUTH_PASSKEY_RP_ID and AUTH_PASSKEY_ORIGIN to match your public site origin (for local: localhost and http://localhost:3008 — not 127.0.0.1)."
                                    </p>
                                    <ol class=STEPS_PREVIEW>
                                        <li><strong class=STEPS_PREVIEW_STRONG>"RP ID"</strong>" must match the site host (no port). Use localhost, never an IP address."</li>
                                        <li><strong class=STEPS_PREVIEW_STRONG>"Origin"</strong>" must match the exact browser origin including scheme and port."</li>
                                        <li><strong class=STEPS_PREVIEW_STRONG>"HTTPS"</strong>" is required outside localhost."</li>
                                    </ol>
                                    <div class=BUTTON_ROW>
                                        <a class=BTN_SECONDARY href="/account/mfa">"Use authenticator app instead"</a>
                                        <a class=BTN_SECONDARY href="/account/password">"Password settings"</a>
                                    </div>
                                </section>
                            }.into_any()
                        } else if !device_ok {
                            view! {
                                <section class=ACCOUNT_PANEL>
                                    <p class=SECTION_LABEL>"Device"</p>
                                    <h2 class=ACCOUNT_PANEL_TITLE>"This browser cannot create passkeys"</h2>
                                    <p class=ACCOUNT_LEDE>
                                        "Try a current Chrome, Safari, Edge, or Firefox build, or open this page on a phone that supports platform authenticators."
                                    </p>
                                    <div class=BUTTON_ROW>
                                        <a class=BTN_SECONDARY href="/account/mfa">"Set up authenticator app"</a>
                                        <a class=BTN_SECONDARY href="/login">"Password sign-in"</a>
                                    </div>
                                </section>
                            }.into_any()
                        } else {
                            view! {
                                <section class=ACCOUNT_PANEL>
                                    <p class=SECTION_LABEL>"Add to this account"</p>
                                    <h2 class=ACCOUNT_PANEL_TITLE>"Create a passkey"</h2>
                                    <ol class=STEPS_PREVIEW>
                                        <li><strong class=STEPS_PREVIEW_STRONG>"Start"</strong>" registration for "{session_email.clone()}"."</li>
                                        <li><strong class=STEPS_PREVIEW_STRONG>"Approve"</strong>" the system prompt (biometrics or security key)."</li>
                                        <li><strong class=STEPS_PREVIEW_STRONG>"Done"</strong>" — use passkey next time you sign in."</li>
                                    </ol>
                                    <button
                                        type="button"
                                        class=with_extra(BTN_PRIMARY, Some(PASSKEY_BUTTON_MT))
                                        disabled=move || start_pending.get() || verify_pending.get() || !can_register
                                        on:click=move |_| {
                                            set_client_error.set(None);
                                            start_action.dispatch(StartPasskeyRegistration {
                                                email: email_dispatch.clone(),
                                                redirect_url: Some("/account/passkeys".to_owned()),
                                            });
                                        }
                                    >"Create a passkey"</button>
                                    <p class=PASSKEY_HINT>
                                        "Works with iCloud Keychain, Google Password Manager, 1Password, and hardware keys (YubiKey, etc.)."
                                    </p>
                                    <p class=BANNER_ERROR hidden=move || client_error.get().is_none()>
                                        {move || client_error.get().unwrap_or_default()}
                                    </p>
                                    <p class=BANNER_ERROR hidden=move || !matches!(start_value.get(), Some(Err(_)))>
                                        {move || match start_value.get() {
                                            Some(Err(error)) => server_error_text(error),
                                            _ => String::new(),
                                        }}
                                    </p>
                                </section>
                            }.into_any()
                        }}
                    </div>
                }.into_any()
            }}
        </div>
    }
}
