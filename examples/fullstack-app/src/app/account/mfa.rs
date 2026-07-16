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
use crate::ui::classes::{
    ACCOUNT_LEDE, ACCOUNT_PANEL, ACCOUNT_PANEL_TITLE, BADGE, BADGE_OFF, BADGE_ON, BANNER_ERROR,
    BTN_PRIMARY, BTN_SECONDARY, BUTTON_ROW, FIELD, INPUT, KV_DD, KV_DT, MFA_ACK, MFA_ACK_LABEL,
    MFA_CODE_INPUT, MFA_COPY_FEEDBACK, MFA_ENROLL_GRID, MFA_ENROLL_SIDE, MFA_FLOW, MFA_FOCUS_PANEL,
    MFA_FOCUS_WRAP, MFA_HINT, MFA_LEDE_WARN, MFA_LINK_DISABLED, MFA_OVERVIEW, MFA_PRIMARY_MT,
    MFA_QR, MFA_QR_CAPTION, MFA_QR_CARD, MFA_RECOVERY_CODE, MFA_RECOVERY_GRID, MFA_SECRET,
    MFA_SECRET_ROW, MFA_STATUS_KV, MFA_VERIFY_TITLE, MONO_VALUE, RESULT_LINE, SECTION_LABEL,
    STATUS_HEAD, STEPS_PREVIEW, STEPS_PREVIEW_STRONG, TEXT_LINK, WIZARD_LINE, WIZARD_LINE_DONE,
    WIZARD_PROGRESS, WIZARD_STEP, WIZARD_STEP_ACTIVE, with_extra,
};
use leptos::prelude::*;
#[cfg(feature = "hydrate")]
use leptos::task::spawn_local;
use server_fn::ServerFnError;

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
        <div class=MFA_FLOW>
            {move || {
                // Recovery codes after confirm — exclusive focus
                if let Some(Ok(value)) = confirm.value().get() {
                    let codes = value.recovery_codes.clone();
                    let codes_for_copy = codes.join("\n");
                    return view! {
                        <div class=MFA_FOCUS_WRAP>
                            <section class=MFA_FOCUS_PANEL>
                                <div class=WIZARD_PROGRESS aria-hidden="true">
                                    <span class=WIZARD_STEP_ACTIVE>"1"</span>
                                    <span class=WIZARD_LINE_DONE></span>
                                    <span class=WIZARD_STEP_ACTIVE>"2"</span>
                                    <span class=WIZARD_LINE_DONE></span>
                                    <span class=WIZARD_STEP_ACTIVE>"3"</span>
                                </div>
                                <span class=format!("{} {}", BADGE, BADGE_ON)>"Authenticator enabled"</span>
                                <h2 class=ACCOUNT_PANEL_TITLE>"Save your recovery codes"</h2>
                                <p class=MFA_LEDE_WARN>
                                    "These codes are the only way back in if you lose your phone. Each code works once. We will not show them again."
                                </p>
                                <ul class=MFA_RECOVERY_GRID>
                                    <For
                                        each=move || codes.clone()
                                        key=|code| code.clone()
                                        children=move |code| view! { <li><code class=MFA_RECOVERY_CODE>{code}</code></li> }
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
                                <label class=MFA_ACK>
                                    <input
                                        type="checkbox"
                                        prop:checked=move || recovery_saved.get()
                                        on:change=move |event| {
                                            set_recovery_saved.set(event_target_checked(&event));
                                        }
                                    />
                                    <span class=MFA_ACK_LABEL>"I stored these recovery codes in a password manager or offline safe place."</span>
                                </label>
                                <p class=MFA_COPY_FEEDBACK hidden=move || copy_feedback.get().is_empty()>
                                    {move || copy_feedback.get()}
                                </p>
                                <a
                                    class=move || {
                                        if recovery_saved.get() {
                                            format!("{} inline-flex w-fit no-underline", BTN_PRIMARY)
                                        } else {
                                            format!("{} inline-flex w-fit no-underline {}", BTN_PRIMARY, MFA_LINK_DISABLED)
                                        }
                                    }
                                    href="/account/mfa"
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
                        <div class=MFA_FOCUS_WRAP>
                            <section class=MFA_FOCUS_PANEL>
                                <div class=WIZARD_PROGRESS aria-hidden="true">
                                    <span class=WIZARD_STEP_ACTIVE>"1"</span>
                                    <span class=WIZARD_LINE></span>
                                    <span class=WIZARD_STEP>"2"</span>
                                    <span class=WIZARD_LINE></span>
                                    <span class=WIZARD_STEP>"3"</span>
                                </div>
                                <p class=SECTION_LABEL>"Step 1 of 3"</p>
                                <h2 class=ACCOUNT_PANEL_TITLE>"Preparing your authenticator setup"</h2>
                                <p class=ACCOUNT_LEDE>"Generating a one-time secret and QR code. Keep this tab open."</p>
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
                        <div class=MFA_FOCUS_WRAP>
                            <section class=MFA_FOCUS_PANEL>
                                <div class=WIZARD_PROGRESS aria-hidden="true">
                                    <span class=WIZARD_STEP_ACTIVE>"1"</span>
                                    <span class=WIZARD_LINE_DONE></span>
                                    <span class=WIZARD_STEP_ACTIVE>"2"</span>
                                    <span class=WIZARD_LINE></span>
                                    <span class=WIZARD_STEP>"3"</span>
                                </div>
                                <p class=SECTION_LABEL>"Step 2 of 3 · Setup only"</p>
                                <h2 class=ACCOUNT_PANEL_TITLE>"Scan this QR code"</h2>
                                <p class=ACCOUNT_LEDE>
                                    "Open your authenticator app, choose add account, then point the camera at this code."
                                </p>
                                <div class=MFA_ENROLL_GRID>
                                    <div class=MFA_QR_CARD>
                                        <div class=MFA_QR inner_html=qr_svg></div>
                                        <p class=MFA_QR_CAPTION>"Works with Google Authenticator, 1Password, Authy, Microsoft Authenticator, and others."</p>
                                    </div>
                                    <div class=MFA_ENROLL_SIDE>
                                        <div class="min-w-0">
                                            <button
                                                type="button"
                                                class=TEXT_LINK
                                                on:click=move |_| set_show_manual_secret.update(|open| *open = !*open)
                                            >
                                                {move || if show_manual_secret.get() {
                                                    "Hide manual entry key"
                                                } else {
                                                    "Can't scan? Enter key manually"
                                                }}
                                            </button>
                                            <div class="min-w-0" hidden=move || !show_manual_secret.get()>
                                                <p class=MFA_HINT>"Type this secret into your app. Spaces are optional."</p>
                                                <div class=MFA_SECRET_ROW>
                                                    <code class=MFA_SECRET>{secret.clone()}</code>
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
                                                <p class=MFA_COPY_FEEDBACK hidden=move || copy_feedback.get().is_empty()>
                                                    {move || copy_feedback.get()}
                                                </p>
                                            </div>
                                        </div>
                                        <div class="min-w-0">
                                            <p class=SECTION_LABEL>"Step 3 of 3"</p>
                                            <h3 class=MFA_VERIFY_TITLE>"Enter the 6-digit code"</h3>
                                            <p class=MFA_HINT>"Your app refreshes a new code about every 30 seconds."</p>
                                            <label class=FIELD>
                                                <span>"Authentication code"</span>
                                                <input
                                                    class=with_extra(INPUT, Some(MFA_CODE_INPUT))
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
                                                class=with_extra(BTN_PRIMARY, Some(MFA_PRIMARY_MT))
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
                        <div class=MFA_OVERVIEW>
                            <section class=ACCOUNT_PANEL>
                                <div class=STATUS_HEAD>
                                    <div>
                                        <p class=SECTION_LABEL>"Security factor"</p>
                                        <h2 class=ACCOUNT_PANEL_TITLE>"Authenticator app (TOTP)"</h2>
                                        <p class=ACCOUNT_LEDE>
                                            "Time-based codes from your authenticator app protect sensitive account actions."
                                        </p>
                                    </div>
                                    <span class=format!("{} {}", BADGE, BADGE_ON)>"Enabled"</span>
                                </div>
                                <dl class=MFA_STATUS_KV>
                                    <dt class=KV_DT>"App codes"</dt>
                                    <dd class=KV_DD>"Ready"</dd>
                                    <dt class=KV_DT>"Recovery codes left"</dt>
                                    <dd class=KV_DD>{move || status.get().and_then(Result::ok).map(|value| value.recovery_codes_remaining.to_string()).unwrap_or_default()}</dd>
                                    <dt class=KV_DT>"Session assurance"</dt>
                                    <dd class=format!("{} {}", KV_DD, MONO_VALUE)>{move || status.get().and_then(Result::ok).map(|value| value.assurance.to_uppercase()).unwrap_or_default()}</dd>
                                </dl>
                            </section>
                            <section class=ACCOUNT_PANEL>
                                <p class=SECTION_LABEL>"This session"</p>
                                <h2 class=ACCOUNT_PANEL_TITLE>"Step up to AAL2"</h2>
                                <p class=ACCOUNT_LEDE>
                                    "Sensitive actions (like changing your password) may require a fresh authenticator code for this browser session."
                                </p>
                                <label class=FIELD>
                                    <span>"Authentication code"</span>
                                    <input
                                        class=with_extra(INPUT, Some(MFA_CODE_INPUT))
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
                                    class=with_extra(BTN_PRIMARY, Some(MFA_PRIMARY_MT))
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
                            <section class=ACCOUNT_PANEL>
                                <p class=SECTION_LABEL>"Backup"</p>
                                <h2 class=ACCOUNT_PANEL_TITLE>"Use a recovery code"</h2>
                                <p class=ACCOUNT_LEDE>
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
                    <div class=MFA_OVERVIEW>
                        <section class=ACCOUNT_PANEL>
                            <div class=STATUS_HEAD>
                                <div>
                                    <p class=SECTION_LABEL>"Security factor"</p>
                                    <h2 class=ACCOUNT_PANEL_TITLE>"Authenticator app (TOTP)"</h2>
                                    <p class=ACCOUNT_LEDE>
                                        "Use Google Authenticator, 1Password, Authy, or any app that supports time-based one-time passwords."
                                    </p>
                                </div>
                                {move || match status.get() {
                                    Some(Ok(value)) if value.totp_enrolled => view! {
                                        <span class=format!("{} {}", BADGE, BADGE_ON)>"Enabled"</span>
                                    }.into_any(),
                                    Some(Ok(_)) => view! {
                                        <span class=format!("{} {}", BADGE, BADGE_OFF)>"Not enabled"</span>
                                    }.into_any(),
                                    Some(Err(_)) => view! {
                                        <span class=format!("{} {}", BADGE, BADGE_OFF)>"Unavailable"</span>
                                    }.into_any(),
                                    None => view! {
                                        <span class=format!("{} {}", BADGE, BADGE_OFF)>"Loading"</span>
                                    }.into_any(),
                                }}
                            </div>
                            <dl class=MFA_STATUS_KV hidden=move || !matches!(status.get(), Some(Ok(_)))>
                                <dt class=KV_DT>"App codes"</dt>
                                <dd class=KV_DD>{move || status.get().and_then(Result::ok).map(|value| if value.totp_enrolled { "Ready" } else { "Not set up" }).unwrap_or_default()}</dd>
                                <dt class=KV_DT>"Recovery codes left"</dt>
                                <dd class=KV_DD>{move || status.get().and_then(Result::ok).map(|value| value.recovery_codes_remaining.to_string()).unwrap_or_default()}</dd>
                                <dt class=KV_DT>"Session assurance"</dt>
                                <dd class=format!("{} {}", KV_DD, MONO_VALUE)>{move || status.get().and_then(Result::ok).map(|value| value.assurance.to_uppercase()).unwrap_or_default()}</dd>
                            </dl>
                            <p class=BANNER_ERROR hidden=move || !matches!(status.get(), Some(Err(_)))>
                                {move || match status.get() {
                                    Some(Err(error)) => server_error_text(error),
                                    _ => String::new(),
                                }}
                            </p>
                        </section>
                        <section class=ACCOUNT_PANEL>
                            <p class=SECTION_LABEL>"Set up"</p>
                            <h2 class=ACCOUNT_PANEL_TITLE>"Add an authenticator"</h2>
                            <ol class=STEPS_PREVIEW>
                                <li><strong class=STEPS_PREVIEW_STRONG>"Install"</strong>" an authenticator app on your phone."</li>
                                <li><strong class=STEPS_PREVIEW_STRONG>"Scan"</strong>" a QR code we show you (or type a secret)."</li>
                                <li><strong class=STEPS_PREVIEW_STRONG>"Enter"</strong>" the 6-digit code the app shows to finish."</li>
                                <li><strong class=STEPS_PREVIEW_STRONG>"Save"</strong>" recovery codes in a safe place — shown once."</li>
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
