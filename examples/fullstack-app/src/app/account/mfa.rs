#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

#[cfg(feature = "hydrate")]
use crate::app::copy_text;
use crate::app::helpers::{action_result_text, server_error_text};
use crate::app::{
    ConfirmTotpEnrollment, GetMfaStatus, StartTotpEnrollment, VerifyRecoveryCode, VerifyTotpStepUp,
    browser_load, confirm_totp_enrollment, get_mfa_status, start_totp_enrollment,
    verify_recovery_code, verify_totp_step_up,
};
use crate::contracts::{
    MfaEnrollConfirmResponse, MfaEnrollStartResponse, MfaStatusResponse, SessionView,
};
use crate::ui::account_page_shell;
use leptos::prelude::*;
#[cfg(feature = "hydrate")]
use leptos::task::spawn_local;
use server_fn::ServerFnError;
use crate::ui::classes::{
    AUTH_TEXT_LINK, BANNER_ERROR, BANNER_SUCCESS, BTN_AUTH_SUBMIT, BTN_PRIMARY, BTN_SECONDARY,
    BUTTON_ROW, FIELD, FIELD_GROUP, INPUT, PANEL, PANEL_COMPACT, RESULT_LINE, SECTION_LABEL,
};

#[component]
pub fn AccountMfaPage() -> impl IntoView {
    account_page_shell(
        "Authenticator app",
        "Protect sign-in with a time-based code from an app you already trust.",
        "mfa",
        view! { <MfaManager /> },
    )
}

/// Standard TOTP enrollment UX (GitHub / Google / Auth0 pattern).
/// Exclusive phases (only one surface mounted at a time):
/// overview → preparing → scan/confirm → recovery codes → enrolled tools.
#[island(lazy)]
pub fn MfaManager() -> impl IntoView {
    let status = browser_load(get_mfa_status);
    let start = ServerAction::<StartTotpEnrollment>::new();
    let confirm = ServerAction::<ConfirmTotpEnrollment>::new();
    let verify = ServerAction::<VerifyTotpStepUp>::new();
    let recover = ServerAction::<VerifyRecoveryCode>::new();
    let (enroll_code, set_enroll_code) = signal(String::new());
    let (step_up_code, set_step_up_code) = signal(String::new());
    let (recovery_code, set_recovery_code) = signal(String::new());
    let (show_manual_secret, set_show_manual_secret) = signal(false);
    let (copy_feedback, set_copy_feedback) = signal(String::new());
    let (recovery_saved, set_recovery_saved) = signal(false);

    view! {
        <div class="mfa-flow">
            {move || {
                // Recovery codes after confirm — exclusive focus
                if let Some(Ok(value)) = confirm.value().get() {
                    let codes = value.recovery_codes.clone();
                    let codes_for_copy = codes.join("\n");
                    return view! {
                        <div class="mfa-flow-focus-wrap">
                            <section class=format!("{}{}", PANEL, " mfa-recovery-panel")>
                                <div class="mfa-wizard-progress" aria-hidden="true">
                                    <span class="mfa-wizard-step is-done">"1"</span>
                                    <span class="mfa-wizard-line is-done"></span>
                                    <span class="mfa-wizard-step is-done">"2"</span>
                                    <span class="mfa-wizard-line is-done"></span>
                                    <span class="mfa-wizard-step is-done">"3"</span>
                                </div>
                                <span class="mfa-badge mfa-badge-on">"Authenticator enabled"</span>
                                <h2>"Save your recovery codes"</h2>
                                <p class="mfa-lede mfa-lede-warn">
                                    "These codes are the only way back in if you lose your phone. Each code works once. We will not show them again."
                                </p>
                                <ul class="mfa-recovery-grid">
                                    <For
                                        each=move || codes.clone()
                                        key=|code| code.clone()
                                        children=move |code| view! { <li><code>{code}</code></li> }
                                    />
                                </ul>
                                <div class=BUTTON_ROW>
                                    <button
                                        type="button"
                                        class=BTN_SECONDARY
                                        on:click=move |_| {
                                            let value = codes_for_copy.clone();
                                            #[cfg(feature = "hydrate")]
                                            {
                                                spawn_local(async move {
                                                    let _ = copy_text(value).await;
                                                    set_copy_feedback.set("Recovery codes copied".to_owned());
                                                });
                                            }
                                            #[cfg(not(feature = "hydrate"))]
                                            {
                                                let _ = value;
                                            }
                                        }
                                    >"Copy all codes"</button>
                                </div>
                                <label class="mfa-ack">
                                    <input
                                        type="checkbox"
                                        prop:checked=move || recovery_saved.get()
                                        on:change=move |event| {
                                            set_recovery_saved.set(event_target_checked(&event));
                                        }
                                    />
                                    <span>"I stored these recovery codes in a password manager or offline safe place."</span>
                                </label>
                                <p class="mfa-hint mfa-copy-feedback" hidden=move || copy_feedback.get().is_empty()>
                                    {move || copy_feedback.get()}
                                </p>
                                <a
                                    class=BTN_PRIMARY
                                    href="/account/mfa"
                                    class:is-disabled=move || !recovery_saved.get()
                                    on:click=move |ev| {
                                        if !recovery_saved.get_untracked() {
                                            ev.prevent_default();
                                            set_copy_feedback.set("Check the box below after you store the codes.".to_owned());
                                        }
                                    }
                                >"Finish setup"</a>
                            </section>
                        </div>
                    }.into_any();
                }

                // Preparing QR — exclusive
                if start.pending().get() {
                    return view! {
                        <div class="mfa-flow-focus-wrap">
                            <section class=format!("{}{}", PANEL, " mfa-enroll-panel mfa-enroll-loading")>
                                <div class="mfa-wizard-progress" aria-hidden="true">
                                    <span class="mfa-wizard-step is-active">"1"</span>
                                    <span class="mfa-wizard-line"></span>
                                    <span class="mfa-wizard-step">"2"</span>
                                    <span class="mfa-wizard-line"></span>
                                    <span class="mfa-wizard-step">"3"</span>
                                </div>
                                <p class=SECTION_LABEL>"Step 1 of 3"</p>
                                <h2>"Preparing your authenticator setup"</h2>
                                <p class="mfa-lede">"Generating a one-time secret and QR code. Keep this tab open."</p>
                                <p class=RESULT_LINE>"Preparing QR code…"</p>
                            </section>
                        </div>
                    }.into_any();
                }

                // Scan + confirm — exclusive (status/intro unmounted)
                if let Some(Ok(enrollment)) = start.value().get() {
                    let secret = enrollment.secret_base32.clone();
                    let uri = enrollment.provisioning_uri.clone();
                    let qr_svg = otpauth_qr_svg(&uri);
                    let secret_for_copy = secret.clone();
                    return view! {
                        <div class="mfa-flow-focus-wrap">
                            <section class=format!("{}{}", PANEL, " mfa-enroll-panel mfa-enroll-focus")>
                                <div class="mfa-wizard-progress" aria-hidden="true">
                                    <span class="mfa-wizard-step is-done">"1"</span>
                                    <span class="mfa-wizard-line is-done"></span>
                                    <span class="mfa-wizard-step is-active">"2"</span>
                                    <span class="mfa-wizard-line"></span>
                                    <span class="mfa-wizard-step">"3"</span>
                                </div>
                                <p class=SECTION_LABEL>"Step 2 of 3 · Setup only"</p>
                                <h2>"Scan this QR code"</h2>
                                <p class="mfa-lede">
                                    "Open your authenticator app, choose add account, then point the camera at this code."
                                </p>
                                <div class="mfa-enroll-grid">
                                    <div class="mfa-qr-card">
                                        <div class="mfa-qr" inner_html=qr_svg></div>
                                        <p class="mfa-qr-caption">"Works with Google Authenticator, 1Password, Authy, Microsoft Authenticator, and others."</p>
                                    </div>
                                    <div class="mfa-enroll-side">
                                        <div class="mfa-manual">
                                            <button
                                                type="button"
                                                class="text-link mfa-manual-toggle"
                                                on:click=move |_| set_show_manual_secret.update(|open| *open = !*open)
                                            >
                                                {move || if show_manual_secret.get() {
                                                    "Hide manual entry key"
                                                } else {
                                                    "Can't scan? Enter key manually"
                                                }}
                                            </button>
                                            <div class="mfa-manual-body" hidden=move || !show_manual_secret.get()>
                                                <p class="mfa-hint">"Type this secret into your app. Spaces are optional."</p>
                                                <div class="mfa-secret-row">
                                                    <code class="mfa-secret">{secret.clone()}</code>
                                                    <button
                                                        type="button"
                                                        class=BTN_SECONDARY
                                                        on:click=move |_| {
                                                            let value = secret_for_copy.clone();
                                                            #[cfg(feature = "hydrate")]
                                                            {
                                                                spawn_local(async move {
                                                                    match copy_text(value).await {
                                                                        Ok(_) => set_copy_feedback.set("Secret copied".to_owned()),
                                                                        Err(_) => set_copy_feedback.set("Copy failed — select the secret manually".to_owned()),
                                                                    }
                                                                });
                                                            }
                                                            #[cfg(not(feature = "hydrate"))]
                                                            {
                                                                let _ = value;
                                                            }
                                                        }
                                                    >"Copy"</button>
                                                </div>
                                                <p class="mfa-hint mfa-copy-feedback" hidden=move || copy_feedback.get().is_empty()>
                                                    {move || copy_feedback.get()}
                                                </p>
                                            </div>
                                        </div>
                                        <div class="mfa-verify-block">
                                            <p class=SECTION_LABEL>"Step 3 of 3"</p>
                                            <h3>"Enter the 6-digit code"</h3>
                                            <p class="mfa-hint">"Your app refreshes a new code about every 30 seconds."</p>
                                            <label class=FIELD>
                                                <span>"Authentication code"</span>
                                                <input
                                                    class=format!("{}{}", INPUT, " mfa-code-input")
                                                    inputmode="numeric"
                                                    autocomplete="one-time-code"
                                                    maxlength="8"
                                                    placeholder="123 456"
                                                    prop:value=move || enroll_code.get()
                                                    on:input=move |event| {
                                                        let raw = event_target_value(&event);
                                                        set_enroll_code.set(raw.chars().filter(|ch| ch.is_ascii_digit()).take(6).collect());
                                                    }
                                                />
                                                <small>"Confirm enrollment before you leave this page."</small>
                                            </label>
                                            <button
                                                type="button"
                                                class=BTN_PRIMARY
                                                disabled=move || confirm.pending().get() || enroll_code.get().len() < 6
                                                on:click=move |_| {
                                                    confirm.dispatch(ConfirmTotpEnrollment {
                                                        code: enroll_code.get_untracked(),
                                                    });
                                                }
                                            >
                                                {move || if confirm.pending().get() { "Verifying…" } else { "Confirm and enable" }}
                                            </button>
                                            <p class=BANNER_ERROR hidden=move || !matches!(confirm.value().get(), Some(Err(_)))>
                                                {move || match confirm.value().get() {
                                                    Some(Err(error)) => server_error_text(error),
                                                    _ => String::new(),
                                                }}
                                            </p>
                                        </div>
                                    </div>
                                </div>
                            </section>
                        </div>
                    }.into_any();
                }

                let enrolled = status
                    .get()
                    .and_then(Result::ok)
                    .is_some_and(|value| value.totp_enrolled);

                // Already enrolled — management tools only
                if enrolled {
                    return view! {
                        <div class="mfa-overview">
                            <section class=format!("{}{}", PANEL, " mfa-status-panel")>
                                <div class="mfa-status-head">
                                    <div>
                                        <p class=SECTION_LABEL>"Security factor"</p>
                                        <h2>"Authenticator app (TOTP)"</h2>
                                        <p class="mfa-lede">
                                            "Time-based codes from your authenticator app protect sensitive account actions."
                                        </p>
                                    </div>
                                    <span class="mfa-badge mfa-badge-on">"Enabled"</span>
                                </div>
                                <dl class="kv mfa-status-kv">
                                    <dt>"App codes"</dt>
                                    <dd>"Ready"</dd>
                                    <dt>"Recovery codes left"</dt>
                                    <dd>{move || status.get().and_then(Result::ok).map(|value| value.recovery_codes_remaining.to_string()).unwrap_or_default()}</dd>
                                    <dt>"Session assurance"</dt>
                                    <dd class="mono-value">{move || status.get().and_then(Result::ok).map(|value| value.assurance.to_uppercase()).unwrap_or_default()}</dd>
                                </dl>
                            </section>
                            <section class=PANEL>
                                <p class=SECTION_LABEL>"This session"</p>
                                <h2>"Step up to AAL2"</h2>
                                <p class="mfa-lede">
                                    "Sensitive actions (like changing your password) may require a fresh authenticator code for this browser session."
                                </p>
                                <label class=FIELD>
                                    <span>"Authentication code"</span>
                                    <input
                                        class=format!("{}{}", INPUT, " mfa-code-input")
                                        inputmode="numeric"
                                        autocomplete="one-time-code"
                                        maxlength="8"
                                        placeholder="123 456"
                                        prop:value=move || step_up_code.get()
                                        on:input=move |event| {
                                            let raw = event_target_value(&event);
                                            set_step_up_code.set(raw.chars().filter(|ch| ch.is_ascii_digit()).take(6).collect());
                                        }
                                    />
                                </label>
                                <button
                                    type="button"
                                    class=BTN_PRIMARY
                                    disabled=move || verify.pending().get() || step_up_code.get().len() < 6
                                    on:click=move |_| {
                                        verify.dispatch(VerifyTotpStepUp {
                                            code: step_up_code.get_untracked(),
                                        });
                                    }
                                >
                                    {move || if verify.pending().get() { "Verifying…" } else { "Verify code" }}
                                </button>
                                <p class=RESULT_LINE hidden=move || verify.value().get().is_none()>
                                    {move || action_result_text(verify.value().get())}
                                </p>
                            </section>
                            <section class=PANEL>
                                <p class=SECTION_LABEL>"Backup"</p>
                                <h2>"Use a recovery code"</h2>
                                <p class="mfa-lede">
                                    "If you cannot open your authenticator app, enter one unused recovery code. That code will be consumed."
                                </p>
                                <label class=FIELD>
                                    <span>"Recovery code"</span>
                                    <input
                                        class=INPUT
                                        autocomplete="one-time-code"
                                        maxlength="32"
                                        prop:value=move || recovery_code.get()
                                        on:input=move |event| set_recovery_code.set(event_target_value(&event).trim().to_owned())
                                    />
                                </label>
                                <button
                                    type="button"
                                    class=BTN_SECONDARY
                                    disabled=move || recover.pending().get() || recovery_code.get().is_empty()
                                    on:click=move |_| {
                                        recover.dispatch(VerifyRecoveryCode {
                                            code: recovery_code.get_untracked(),
                                        });
                                    }
                                >"Use recovery code"</button>
                                <p class=RESULT_LINE hidden=move || recover.value().get().is_none()>
                                    {move || action_result_text(recover.value().get())}
                                </p>
                            </section>
                        </div>
                    }.into_any();
                }

                // Default overview: status + setup CTA only
                view! {
                    <div class="mfa-overview">
                        <section class=format!("{}{}", PANEL, " mfa-status-panel")>
                            <div class="mfa-status-head">
                                <div>
                                    <p class=SECTION_LABEL>"Security factor"</p>
                                    <h2>"Authenticator app (TOTP)"</h2>
                                    <p class="mfa-lede">
                                        "Use Google Authenticator, 1Password, Authy, or any app that supports time-based one-time passwords."
                                    </p>
                                </div>
                                {move || match status.get() {
                                    Some(Ok(value)) if value.totp_enrolled => view! {
                                        <span class="mfa-badge mfa-badge-on">"Enabled"</span>
                                    }.into_any(),
                                    Some(Ok(_)) => view! {
                                        <span class="mfa-badge mfa-badge-off">"Not enabled"</span>
                                    }.into_any(),
                                    Some(Err(_)) => view! {
                                        <span class="mfa-badge mfa-badge-off">"Unavailable"</span>
                                    }.into_any(),
                                    None => view! {
                                        <span class="mfa-badge mfa-badge-off">"Loading"</span>
                                    }.into_any(),
                                }}
                            </div>
                            <dl class="kv mfa-status-kv" hidden=move || !matches!(status.get(), Some(Ok(_)))>
                                <dt>"App codes"</dt>
                                <dd>{move || status.get().and_then(Result::ok).map(|value| if value.totp_enrolled { "Ready" } else { "Not set up" }).unwrap_or_default()}</dd>
                                <dt>"Recovery codes left"</dt>
                                <dd>{move || status.get().and_then(Result::ok).map(|value| value.recovery_codes_remaining.to_string()).unwrap_or_default()}</dd>
                                <dt>"Session assurance"</dt>
                                <dd class="mono-value">{move || status.get().and_then(Result::ok).map(|value| value.assurance.to_uppercase()).unwrap_or_default()}</dd>
                            </dl>
                            <p class=BANNER_ERROR hidden=move || !matches!(status.get(), Some(Err(_)))>
                                {move || match status.get() {
                                    Some(Err(error)) => server_error_text(error),
                                    _ => String::new(),
                                }}
                            </p>
                        </section>
                        <section class=PANEL>
                            <p class=SECTION_LABEL>"Set up"</p>
                            <h2>"Add an authenticator"</h2>
                            <ol class="mfa-steps-preview">
                                <li><strong>"Install"</strong>" an authenticator app on your phone."</li>
                                <li><strong>"Scan"</strong>" a QR code we show you (or type a secret)."</li>
                                <li><strong>"Enter"</strong>" the 6-digit code the app shows to finish."</li>
                                <li><strong>"Save"</strong>" recovery codes in a safe place — shown once."</li>
                            </ol>
                            <button
                                type="button"
                                class=BTN_PRIMARY
                                disabled=move || start.pending().get()
                                on:click=move |_| {
                                    set_show_manual_secret.set(false);
                                    set_enroll_code.set(String::new());
                                    set_copy_feedback.set(String::new());
                                    start.dispatch(StartTotpEnrollment {});
                                }
                            >"Set up authenticator"</button>
                            <p class=BANNER_ERROR hidden=move || !matches!(start.value().get(), Some(Err(_)))>
                                {move || match start.value().get() {
                                    Some(Err(error)) => server_error_text(error),
                                    _ => String::new(),
                                }}
                            </p>
                        </section>
                    </div>
                }.into_any()
            }}
        </div>
    }
}

pub fn otpauth_qr_svg(uri: &str) -> String {
    #[cfg(feature = "hydrate")]
    {
        use qrcode::QrCode;
        use qrcode::render::svg;
        match QrCode::new(uri.as_bytes()) {
            Ok(code) => code
                .render::<svg::Color>()
                .min_dimensions(168, 168)
                .dark_color(svg::Color("#0d0d0d"))
                .light_color(svg::Color("#ffffff"))
                .quiet_zone(true)
                .build(),
            Err(_) => String::new(),
        }
    }
    #[cfg(not(feature = "hydrate"))]
    {
        let _ = uri;
        String::new()
    }
}
