//! Account settings: profile, password, MFA, passkeys, sessions, providers, vault.

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use crate::app::helpers::{
    action_result_text, is_passkey_cancel_message, optional_text, redirect_browser,
    server_error_text, short_id_label,
};
#[cfg(feature = "hydrate")]
use crate::app::helpers::{passkey_js_error, passkey_js_string};
use crate::app::{
    browser_load, change_password, confirm_totp_enrollment, create_dashboard_secret,
    delete_dashboard_secret, get_account_profile, get_auth_capabilities, get_current_session,
    get_mfa_status, get_public_profile, list_account_sessions, list_auth_providers,
    list_dashboard_secrets, resolve_workspace_vault_target, reveal_dashboard_secret,
    revoke_account_session, seed_dashboard_demos, start_passkey_registration, start_totp_enrollment,
    update_account_profile, verify_passkey_registration, verify_recovery_code, verify_totp_step_up,
    ChangePassword, ConfirmTotpEnrollment, CreateDashboardSecret, DeleteDashboardSecret,
    GetAccountProfile, GetAuthCapabilities, GetMfaStatus, GetPublicProfile, ListAccountSessions,
    ListAuthProviders, ListDashboardSecrets, ResolveWorkspaceVaultTarget, RevealDashboardSecret,
    RevokeAccountSession, SeedDashboardDemos, StartPasskeyRegistration, StartTotpEnrollment,
    UpdateAccountProfile, VerifyPasskeyRegistration, VerifyRecoveryCode, VerifyTotpStepUp,
};
use crate::contracts::{
    AccountSessionSummary, AuthProviderSummary, MfaEnrollConfirmResponse, MfaEnrollStartResponse,
    MfaStatusResponse, ProfileUpdateRequest, ProfileView, PublicProfileView, SecretCreateRequest,
    SecretSummary, SessionView,
};
use crate::ui::{
    account_page_shell, page_shell, public_page_shell, ErrorBanner, Field, FieldGroup, Panel,
    PrimaryButton, SectionLabel, SuccessBanner, TextInput,
};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use server_fn::ServerFnError;
#[cfg(feature = "hydrate")]
use leptos::task::spawn_local;
#[cfg(feature = "hydrate")]
use wasm_bindgen::prelude::*;
#[cfg(feature = "hydrate")]
use web_sys::window;

#[cfg(feature = "hydrate")]
use crate::app::{copy_text, create_passkey_credential, passkey_supported, pick_image_data_url};

/// Known OAuth brands for the account Providers tab (always shown; greyed when off).
#[derive(Clone, Copy)]
pub(crate) struct ProviderBrand {
    id: &'static str,
    name: &'static str,
}

pub(crate) const PROVIDER_CATALOG: &[ProviderBrand] = &[
    ProviderBrand {
        id: "google",
        name: "Google",
    },
    ProviderBrand {
        id: "facebook",
        name: "Facebook",
    },
    ProviderBrand {
        id: "apple",
        name: "Apple",
    },
];


#[component]
pub fn AccountProfilePage() -> impl IntoView {
    account_page_shell(
        "Profile",
        "Your name, @handle, avatar, and whether others can find you.",
        "profile",
        view! { <AccountProfileCard /> },
    )
}


#[component]
pub fn PublicProfilePage() -> impl IntoView {
    let params = use_params_map();
    public_page_shell(
        "Profile",
        "Public account",
        view! {
            {move || {
                let handle = params
                    .get()
                    .get("handle")
                    .map(|value| value.to_string())
                    .unwrap_or_default();
                view! { <PublicProfileCard handle=handle /> }.into_any()
            }}
        },
    )
}


#[component]
pub fn AccountPasswordPage() -> impl IntoView {
    account_page_shell(
        "Password",
        "Update the password for this account. Enter your current password to confirm.",
        "password",
        view! { <ChangePasswordForm /> },
    )
}


#[island(lazy)]
pub fn AccountProvidersPage() -> impl IntoView {
    account_page_shell(
        "Providers",
        "Social sign-in options for this deployment. Enabled providers can be used on the login page.",
        "providers",
        view! { <AccountProvidersPanel /> },
    )
}


#[island(lazy)]
pub fn AccountProvidersPanel() -> impl IntoView {
    let providers = browser_load(list_auth_providers);

    view! {
        <section class="panel providers-panel">
            <div class="session-panel-head">
                <div>
                    <p class="section-label">"Social login"</p>
                    <h2>"Identity providers"</h2>
                </div>
            </div>
            <p class="passkey-lede">
                "These providers appear on the sign-in page when credentials are configured and OAuth is enabled. Greyed tiles are available but not active on this deployment."
            </p>

            <div class="provider-catalog">
                {PROVIDER_CATALOG
                    .iter()
                    .copied()
                    .map(|brand| {
                        view! {
                            <ProviderCatalogCard brand=brand providers=providers />
                        }
                    })
                    .collect_view()}
            </div>

            <p class="providers-empty-note">
                {move || match providers.get() {
                    None => "Loading provider status…".to_string(),
                    Some(Ok(list)) if list.is_empty() => {
                        "No providers are enabled.".to_string()
                    }
                    Some(Ok(list)) => {
                        let n = list.iter().filter(|p| p.enabled).count();
                        if n == 0 {
                            "No providers are enabled.".to_string()
                        } else {
                            format!(
                                "{n} provider{} enabled for sign-in.",
                                if n == 1 { "" } else { "s" }
                            )
                        }
                    }
                    Some(Err(error)) => server_error_text(error),
                }}
            </p>
        </section>
    }
}


#[component]
pub fn ProviderCatalogCard(
    brand: ProviderBrand,
    providers: ReadSignal<Option<Result<Vec<AuthProviderSummary>, ServerFnError>>>,
) -> impl IntoView {
    let brand_id = brand.id;
    let brand_name = brand.name;
    let is_enabled = move || {
        providers.get().and_then(Result::ok).is_some_and(|list| {
            list.iter()
                .any(|p| p.provider_id.eq_ignore_ascii_case(brand_id) && p.enabled)
        })
    };

    view! {
        <div
            class="provider-card"
            class:is-enabled=move || is_enabled()
            class:is-disabled=move || !is_enabled()
            data-provider=brand_id
        >
            <span class="provider-logo" aria-hidden="true" inner_html=provider_logo_svg(brand_id)></span>
            <span class="provider-card-body">
                <span class="provider-name">{brand_name}</span>
                <span class="provider-status">
                    {move || if is_enabled() { "Enabled" } else { "Not configured" }}
                </span>
            </span>
        </div>
    }
}


pub fn provider_logo_svg(provider_id: &str) -> String {
    // Simple monochrome brand marks; CSS greys them when disabled.
    match provider_id {
        "google" => r#"<svg viewBox="0 0 24 24" width="28" height="28" xmlns="http://www.w3.org/2000/svg" fill="currentColor" aria-hidden="true"><path d="M21.35 11.1h-9.18v2.96h5.27c-.23 1.5-1.72 4.4-5.27 4.4-3.17 0-5.76-2.62-5.76-5.86s2.59-5.86 5.76-5.86c1.8 0 3.01.77 3.7 1.43l2.52-2.43C16.99 4.33 15.03 3.4 12.17 3.4 6.99 3.4 2.8 7.58 2.8 12.6s4.19 9.2 9.37 9.2c5.41 0 8.99-3.8 8.99-9.15 0-.61-.07-1.08-.16-1.55z"/></svg>"#.to_owned(),
        "facebook" => r#"<svg viewBox="0 0 24 24" width="28" height="28" xmlns="http://www.w3.org/2000/svg" fill="currentColor" aria-hidden="true"><path d="M13.5 22v-8.1h2.72l.41-3.17h-3.13V8.7c0-.92.25-1.54 1.57-1.54H16.8V4.32C16.4 4.27 15.2 4.16 13.8 4.16c-2.9 0-4.88 1.77-4.88 5.02v2.8H6.2v3.17h2.72V22h4.58z"/></svg>"#.to_owned(),
        "apple" => r#"<svg viewBox="0 0 24 24" width="28" height="28" xmlns="http://www.w3.org/2000/svg" fill="currentColor" aria-hidden="true"><path d="M16.37 12.64c.02 2.3 2.02 3.07 2.04 3.08-.02.06-.32 1.1-1.05 2.18-.63.93-1.29 1.86-2.32 1.88-1.01.02-1.34-.6-2.5-.6-1.16 0-1.52.58-2.48.62-1 .04-1.76-.98-2.4-1.91-1.31-1.9-2.31-5.37-1-7.72.68-1.21 1.9-1.98 3.22-2 1-.02 1.95.68 2.5.68.55 0 1.8-.84 3.03-.71.52.02 1.97.21 2.9 1.58-.08.05-1.73 1.01-1.72 3.02zM14.9 6.5c.54-.66.91-1.57.81-2.48-.78.03-1.73.52-2.29 1.18-.5.58-.94 1.51-.82 2.4.87.07 1.76-.44 2.3-1.1z"/></svg>"#.to_owned(),
        _ => r#"<svg viewBox="0 0 24 24" width="28" height="28" xmlns="http://www.w3.org/2000/svg" fill="currentColor" aria-hidden="true"><circle cx="12" cy="12" r="9"/></svg>"#.to_owned(),
    }
}


#[component]
pub fn AccountPasskeysPage() -> impl IntoView {
    account_page_shell(
        "Passkeys",
        "Sign in with Face ID, Touch ID, Windows Hello, or a security key — no password to type.",
        "passkeys",
        view! { <PasskeyManager /> },
    )
}


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
                            <section class="panel mfa-recovery-panel">
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
                                <div class="button-row">
                                    <button
                                        type="button"
                                        class="secondary-button"
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
                                    class="primary-button"
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
                            <section class="panel mfa-enroll-panel mfa-enroll-loading">
                                <div class="mfa-wizard-progress" aria-hidden="true">
                                    <span class="mfa-wizard-step is-active">"1"</span>
                                    <span class="mfa-wizard-line"></span>
                                    <span class="mfa-wizard-step">"2"</span>
                                    <span class="mfa-wizard-line"></span>
                                    <span class="mfa-wizard-step">"3"</span>
                                </div>
                                <p class="section-label">"Step 1 of 3"</p>
                                <h2>"Preparing your authenticator setup"</h2>
                                <p class="mfa-lede">"Generating a one-time secret and QR code. Keep this tab open."</p>
                                <p class="result-line">"Preparing QR code…"</p>
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
                            <section class="panel mfa-enroll-panel mfa-enroll-focus">
                                <div class="mfa-wizard-progress" aria-hidden="true">
                                    <span class="mfa-wizard-step is-done">"1"</span>
                                    <span class="mfa-wizard-line is-done"></span>
                                    <span class="mfa-wizard-step is-active">"2"</span>
                                    <span class="mfa-wizard-line"></span>
                                    <span class="mfa-wizard-step">"3"</span>
                                </div>
                                <p class="section-label">"Step 2 of 3 · Setup only"</p>
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
                                                        class="secondary-button"
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
                                            <p class="section-label">"Step 3 of 3"</p>
                                            <h3>"Enter the 6-digit code"</h3>
                                            <p class="mfa-hint">"Your app refreshes a new code about every 30 seconds."</p>
                                            <label class="auth-field">
                                                <span>"Authentication code"</span>
                                                <input
                                                    class="auth-input mfa-code-input"
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
                                                class="primary-button"
                                                disabled=move || confirm.pending().get() || enroll_code.get().len() < 6
                                                on:click=move |_| {
                                                    confirm.dispatch(ConfirmTotpEnrollment {
                                                        code: enroll_code.get_untracked(),
                                                    });
                                                }
                                            >
                                                {move || if confirm.pending().get() { "Verifying…" } else { "Confirm and enable" }}
                                            </button>
                                            <p class="error-banner" hidden=move || !matches!(confirm.value().get(), Some(Err(_)))>
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
                            <section class="panel mfa-status-panel">
                                <div class="mfa-status-head">
                                    <div>
                                        <p class="section-label">"Security factor"</p>
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
                            <section class="panel">
                                <p class="section-label">"This session"</p>
                                <h2>"Step up to AAL2"</h2>
                                <p class="mfa-lede">
                                    "Sensitive actions (like changing your password) may require a fresh authenticator code for this browser session."
                                </p>
                                <label class="auth-field">
                                    <span>"Authentication code"</span>
                                    <input
                                        class="auth-input mfa-code-input"
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
                                    class="primary-button"
                                    disabled=move || verify.pending().get() || step_up_code.get().len() < 6
                                    on:click=move |_| {
                                        verify.dispatch(VerifyTotpStepUp {
                                            code: step_up_code.get_untracked(),
                                        });
                                    }
                                >
                                    {move || if verify.pending().get() { "Verifying…" } else { "Verify code" }}
                                </button>
                                <p class="result-line" hidden=move || verify.value().get().is_none()>
                                    {move || action_result_text(verify.value().get())}
                                </p>
                            </section>
                            <section class="panel">
                                <p class="section-label">"Backup"</p>
                                <h2>"Use a recovery code"</h2>
                                <p class="mfa-lede">
                                    "If you cannot open your authenticator app, enter one unused recovery code. That code will be consumed."
                                </p>
                                <label class="auth-field">
                                    <span>"Recovery code"</span>
                                    <input
                                        class="auth-input"
                                        autocomplete="one-time-code"
                                        maxlength="32"
                                        prop:value=move || recovery_code.get()
                                        on:input=move |event| set_recovery_code.set(event_target_value(&event).trim().to_owned())
                                    />
                                </label>
                                <button
                                    type="button"
                                    class="secondary-button"
                                    disabled=move || recover.pending().get() || recovery_code.get().is_empty()
                                    on:click=move |_| {
                                        recover.dispatch(VerifyRecoveryCode {
                                            code: recovery_code.get_untracked(),
                                        });
                                    }
                                >"Use recovery code"</button>
                                <p class="result-line" hidden=move || recover.value().get().is_none()>
                                    {move || action_result_text(recover.value().get())}
                                </p>
                            </section>
                        </div>
                    }.into_any();
                }

                // Default overview: status + setup CTA only
                view! {
                    <div class="mfa-overview">
                        <section class="panel mfa-status-panel">
                            <div class="mfa-status-head">
                                <div>
                                    <p class="section-label">"Security factor"</p>
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
                            <p class="error-banner" hidden=move || !matches!(status.get(), Some(Err(_)))>
                                {move || match status.get() {
                                    Some(Err(error)) => server_error_text(error),
                                    _ => String::new(),
                                }}
                            </p>
                        </section>
                        <section class="panel">
                            <p class="section-label">"Set up"</p>
                            <h2>"Add an authenticator"</h2>
                            <ol class="mfa-steps-preview">
                                <li><strong>"Install"</strong>" an authenticator app on your phone."</li>
                                <li><strong>"Scan"</strong>" a QR code we show you (or type a secret)."</li>
                                <li><strong>"Enter"</strong>" the 6-digit code the app shows to finish."</li>
                                <li><strong>"Save"</strong>" recovery codes in a safe place — shown once."</li>
                            </ol>
                            <button
                                type="button"
                                class="primary-button"
                                disabled=move || start.pending().get()
                                on:click=move |_| {
                                    set_show_manual_secret.set(false);
                                    set_enroll_code.set(String::new());
                                    set_copy_feedback.set(String::new());
                                    start.dispatch(StartTotpEnrollment {});
                                }
                            >"Set up authenticator"</button>
                            <p class="error-banner" hidden=move || !matches!(start.value().get(), Some(Err(_)))>
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
        use qrcode::render::svg;
        use qrcode::QrCode;
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


#[component]
pub fn AccountSessionsPage() -> impl IntoView {
    account_page_shell(
        "Sessions",
        "Review and revoke browser access for this account.",
        "sessions",
        view! { <AccountSessionManager /> },
    )
}


/// Legacy `/account/vault` → selected org vault or onboarding.
#[component]
pub fn AccountVaultRedirectPage() -> impl IntoView {
    page_shell(
        "Secret vault",
        "Opening your workspace vault…",
        view! { <AccountVaultRedirect /> },
    )
}


#[island]
pub fn AccountVaultRedirect() -> impl IntoView {
    let target = browser_load(resolve_workspace_vault_target);
    Effect::new(move |_| match target.get() {
        Some(Ok(org)) if !org.slug.is_empty() => {
            redirect_browser(&format!("/org/{}/vault", org.slug));
        }
        Some(Ok(_)) | Some(Err(_)) => {
            redirect_browser("/onboarding/workspace");
        }
        None => {}
    });
    view! {
        <section class="panel">
            <p class="result-line">"Redirecting to your workspace vault…"</p>
            <p class="board-muted">
                <a href="/onboarding/workspace">"Create a workspace"</a>
                " if you do not have one yet."
            </p>
        </section>
    }
}


#[component]
pub fn OrgVaultPage() -> impl IntoView {
    let params = use_params_map();
    account_page_shell(
        "Secret vault",
        "Encrypted at rest. Values are never shown in lists. Use keys like STRIPE_SECRET_KEY in connectors.",
        "vault",
        view! {
            {move || {
                let slug = params
                    .get()
                    .get("slug")
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                view! { <AccountVaultPanel org_slug=slug /> }.into_any()
            }}
        },
    )
}


#[island]
pub fn AccountVaultPanel(org_slug: String) -> impl IntoView {
    let org_slug = RwSignal::new(org_slug);
    let org_slug_for_load = org_slug.get_untracked();
    let secrets_res = browser_load(move || list_dashboard_secrets(org_slug_for_load.clone()));
    let secrets = RwSignal::new(Vec::<crate::contracts::SecretSummary>::new());
    let form_error = RwSignal::new(None::<String>);
    let form_ok = RwSignal::new(None::<String>);
    let key = RwSignal::new(String::new());
    let label = RwSignal::new(String::new());
    let description = RwSignal::new(String::new());
    let value = RwSignal::new(String::new());
    let show_value = RwSignal::new(false);
    let create_open = RwSignal::new(false);
    let delete_target = RwSignal::new(None::<(String, String)>); // (id, key)
    let revealed = RwSignal::new(std::collections::HashMap::<String, String>::new());
    let reveal_pending = RwSignal::new(None::<String>);

    let create_action = ServerAction::<CreateDashboardSecret>::new();
    let delete_action = ServerAction::<DeleteDashboardSecret>::new();
    let reveal_action = ServerAction::<RevealDashboardSecret>::new();
    let seed_action = ServerAction::<SeedDashboardDemos>::new();

    Effect::new(move |_| {
        if let Some(Ok(list)) = secrets_res.get() {
            secrets.set(list);
        } else if let Some(Err(e)) = secrets_res.get() {
            form_error.set(Some(e.to_string()));
        }
    });

    Effect::new(move |_| match create_action.value().get() {
        Some(Ok(summary)) => {
            secrets.update(|list| {
                if !list.iter().any(|s| s.id == summary.id) {
                    list.push(summary);
                }
            });
            key.set(String::new());
            label.set(String::new());
            description.set(String::new());
            value.set(String::new());
            show_value.set(false);
            create_open.set(false);
            form_error.set(None);
            form_ok.set(Some("Secret stored. Value is encrypted and hidden.".into()));
        }
        Some(Err(e)) => {
            form_ok.set(None);
            form_error.set(Some(e.to_string()));
        }
        None => {}
    });

    Effect::new(move |_| match delete_action.value().get() {
        Some(Ok(_)) => {
            if let Some((id, _)) = delete_target.get_untracked() {
                secrets.update(|l| l.retain(|s| s.id != id));
                revealed.update(|m| {
                    m.remove(&id);
                });
            }
            delete_target.set(None);
            form_error.set(None);
            form_ok.set(Some("Secret deleted.".into()));
        }
        Some(Err(e)) => {
            form_ok.set(None);
            form_error.set(Some(e.to_string()));
        }
        None => {}
    });

    Effect::new(move |_| match reveal_action.value().get() {
        Some(Ok(resp)) => {
            let ttl = resp.reveal_ttl_seconds.max(5) as u64;
            revealed.update(|map| {
                map.insert(resp.id.clone(), resp.value);
            });
            reveal_pending.set(None);
            form_error.set(None);
            #[cfg(feature = "hydrate")]
            {
                let id_hide = resp.id.clone();
                spawn_local(async move {
                    gloo_timers_sleep_ms(ttl.saturating_mul(1000)).await;
                    revealed.update(|map| {
                        map.remove(&id_hide);
                    });
                });
            }
            let _ = ttl;
        }
        Some(Err(e)) => {
            reveal_pending.set(None);
            let msg = e.to_string();
            if msg.to_ascii_lowercase().contains("forbidden")
                || msg.to_ascii_lowercase().contains("step")
            {
                form_error.set(Some(
                    "Reveal requires step-up (AAL2). Complete MFA on /account/mfa, then try again."
                        .into(),
                ));
            } else {
                form_error.set(Some(msg));
            }
        }
        None => {}
    });

    Effect::new(move |_| {
        if let Some(Ok(_)) = seed_action.value().get() {
            form_ok.set(Some(
                "Demo connectors seeded. Open the dashboard to see bound widgets.".into(),
            ));
            form_error.set(None);
            #[cfg(feature = "hydrate")]
            {
                let slug = org_slug.get_untracked();
                spawn_local(async move {
                    if let Ok(list) = list_dashboard_secrets(slug).await {
                        secrets.set(list);
                    }
                });
            }
        }
    });

    let open_create_modal = move || {
        form_error.set(None);
        form_ok.set(None);
        key.set(String::new());
        label.set(String::new());
        description.set(String::new());
        value.set(String::new());
        show_value.set(false);
        create_open.set(true);
    };

    view! {
        <div class="vault-page">
            <section class="panel vault-intro">
                <p class="section-label">"Connectors · Integrations"</p>
                <p class="vault-lede">
                    "Store API keys and passwords for REST, Postgres, and future integrations. "
                    "Resource pickers reference keys by id — plaintext never appears in list APIs."
                </p>
                <div class="vault-actions">
                    <a class="secondary-button" href="/dashboard">"Back to dashboard"</a>
                    <button
                        type="button"
                        class="secondary-button"
                        disabled=move || seed_action.pending().get()
                        on:click=move |_| { seed_action.dispatch(SeedDashboardDemos {}); }
                    >
                        {move || if seed_action.pending().get() { "Loading demos…" } else { "Load demo connectors" }}
                    </button>
                </div>
            </section>

            <p class="error-banner" hidden=move || form_error.get().is_none()>
                {move || form_error.get().unwrap_or_default()}
            </p>
            <p class="success-banner" hidden=move || form_ok.get().is_none()>
                {move || form_ok.get().unwrap_or_default()}
            </p>

            <section class="panel vault-list-panel">
                <header class="vault-panel-head">
                    <h2>"Secrets"</h2>
                    <div class="vault-panel-head-meta">
                        <span class="board-muted">{move || format!("{} stored", secrets.get().len())}</span>
                        <button type="button" class="secondary-button vault-add-inline" on:click=move |_| open_create_modal()>
                            "Add secret"
                        </button>
                    </div>
                </header>
                <div class="vault-table-wrap">
                    <table class="vault-table">
                        <colgroup>
                            <col class="vault-col-key" />
                            <col class="vault-col-label" />
                            <col class="vault-col-scope" />
                            <col class="vault-col-value" />
                            <col class="vault-col-actions" />
                        </colgroup>
                        <thead>
                            <tr>
                                <th scope="col">"Key"</th>
                                <th scope="col">"Label"</th>
                                <th scope="col">"Scope"</th>
                                <th scope="col">"Value"</th>
                                <th scope="col" class="vault-th-actions"><span class="sr-only">"Actions"</span></th>
                            </tr>
                        </thead>
                        <tbody>
                            {move || {
                                let list = secrets.get();
                                if list.is_empty() {
                                    return view! {
                                        <tr>
                                            <td colspan="5" class="board-muted vault-empty">
                                                "No secrets yet. Use Add secret above."
                                            </td>
                                        </tr>
                                    }.into_any();
                                }
                                list.into_iter().map(|sec| {
                                    let id = sec.id.clone();
                                    let id_for_reveal = sec.id.clone();
                                    let id_for_pending = sec.id.clone();
                                    let id_del = sec.id.clone();
                                    let key_label = if sec.key.is_empty() { sec.name.clone() } else { sec.key.clone() };
                                    let key_for_delete = key_label.clone();
                                    let label_text = sec.label.clone();
                                    let scope = sec.scope.clone();
                                    let masked = sec.masked_value.clone();
                                    view! {
                                        <tr>
                                            <td class="mono-value vault-td-key">{key_label}</td>
                                            <td class="vault-td-label">{label_text}</td>
                                            <td class="vault-td-scope"><span class="vault-scope">{scope}</span></td>
                                            <td class="vault-td-value">
                                                <div class="vault-value-inner">
                                                    {move || {
                                                        let id_check = id.clone();
                                                        let revealed_map = revealed.get();
                                                        if let Some(plain) = revealed_map.get(&id_check).cloned() {
                                                            view! {
                                                                <code class="vault-revealed">{plain}</code>
                                                            }.into_any()
                                                        } else {
                                                            let id_click = id_for_reveal.clone();
                                                            let id_pend = id_for_pending.clone();
                                                            let masked_show = masked.clone();
                                                            view! {
                                                                <span class="vault-masked">{masked_show}</span>
                                                                <button
                                                                    type="button"
                                                                    class="vault-eye"
                                                                    aria-label="Reveal secret"
                                                                    disabled={
                                                                        let id_pend = id_pend.clone();
                                                                        move || {
                                                                            reveal_pending.get().as_ref() == Some(&id_pend)
                                                                                || reveal_action.pending().get()
                                                                        }
                                                                    }
                                                                    on:click=move |_| {
                                                                        reveal_pending.set(Some(id_click.clone()));
                                                                        reveal_action.dispatch(RevealDashboardSecret {
                                                                            org_slug: org_slug.get_untracked(),
                                                                            secret_id: id_click.clone(),
                                                                        });
                                                                    }
                                                                >"👁"</button>
                                                            }.into_any()
                                                        }
                                                    }}
                                                </div>
                                            </td>
                                            <td class="vault-td-actions">
                                                <button
                                                    type="button"
                                                    class="vault-trash"
                                                    aria-label=format!("Delete secret {key_for_delete}")
                                                    title="Delete secret"
                                                    on:click=move |_| {
                                                        form_ok.set(None);
                                                        delete_target.set(Some((id_del.clone(), key_for_delete.clone())));
                                                    }
                                                >
                                                    <svg class="vault-trash-icon" viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">
                                                        <path d="M3 6h18" />
                                                        <path d="M8 6V4h8v2" />
                                                        <path d="M19 6l-1 14H6L5 6" />
                                                        <path d="M10 11v6" />
                                                        <path d="M14 11v6" />
                                                    </svg>
                                                </button>
                                            </td>
                                        </tr>
                                    }
                                }).collect_view().into_any()
                            }}
                        </tbody>
                    </table>
                </div>
            </section>

            // Create secret modal
            <Show when=move || create_open.get()>
                <div
                    class="board-modal-backdrop vault-modal-backdrop"
                    role="presentation"
                    on:click=move |_| create_open.set(false)
                    on:wheel=move |e| e.stop_propagation()
                >
                    <div
                        class="board-modal vault-modal"
                        role="dialog"
                        aria-modal="true"
                        aria-labelledby="vault-create-title"
                        on:click=move |e| e.stop_propagation()
                    >
                        <header class="board-modal-head">
                            <div>
                                <h2 id="vault-create-title">"Add secret"</h2>
                                <p>"Keys look like environment variables. Values are encrypted with AUTH_VAULT_KEY before storage."</p>
                            </div>
                            <button type="button" class="board-modal-close" on:click=move |_| create_open.set(false)>"Close"</button>
                        </header>
                        <div class="board-modal-body vault-modal-body">
                            <div class="vault-form vault-form-modal">
                                <label class="auth-field">
                                    <span>"Key"</span>
                                    <input
                                        class="auth-input mono-value"
                                        placeholder="STRIPE_SECRET_KEY"
                                        prop:value=move || key.get()
                                        on:input=move |e| key.set(event_target_value(&e).to_ascii_uppercase())
                                    />
                                </label>
                                <label class="auth-field">
                                    <span>"Label"</span>
                                    <input
                                        class="auth-input"
                                        placeholder="Stripe live secret"
                                        prop:value=move || label.get()
                                        on:input=move |e| label.set(event_target_value(&e))
                                    />
                                </label>
                                <label class="auth-field vault-field-wide">
                                    <span>"Description (optional)"</span>
                                    <input
                                        class="auth-input"
                                        prop:value=move || description.get()
                                        on:input=move |e| description.set(event_target_value(&e))
                                    />
                                </label>
                                <label class="auth-field vault-field-wide">
                                    <span>"Value"</span>
                                    <div class="vault-value-input-row">
                                        <input
                                            class="auth-input"
                                            type=move || if show_value.get() { "text" } else { "password" }
                                            autocomplete="new-password"
                                            prop:value=move || value.get()
                                            on:input=move |e| value.set(event_target_value(&e))
                                        />
                                        <button
                                            type="button"
                                            class="secondary-button"
                                            on:click=move |_| show_value.update(|v| *v = !*v)
                                        >
                                            {move || if show_value.get() { "Hide" } else { "Show" }}
                                        </button>
                                    </div>
                                </label>
                            </div>
                            <div class="vault-modal-actions">
                                <button type="button" class="secondary-button" on:click=move |_| create_open.set(false)>"Cancel"</button>
                                <button
                                    type="button"
                                    class="primary-button"
                                    disabled=move || create_action.pending().get()
                                    on:click=move |_| {
                                        form_ok.set(None);
                                        form_error.set(None);
                                        create_action.dispatch(CreateDashboardSecret {
                                            org_slug: org_slug.get_untracked(),
                                            request: SecretCreateRequest {
                                                key: key.get_untracked(),
                                                name: key.get_untracked(),
                                                value: value.get_untracked(),
                                                label: label.get_untracked(),
                                                description: description.get_untracked(),
                                                scope: "user".to_owned(),
                                            },
                                        });
                                    }
                                >
                                    {move || if create_action.pending().get() { "Encrypting…" } else { "Store secret" }}
                                </button>
                            </div>
                        </div>
                    </div>
                </div>
            </Show>

            // Delete confirmation modal
            <Show when=move || delete_target.get().is_some()>
                <div
                    class="board-modal-backdrop vault-modal-backdrop"
                    role="presentation"
                    on:click=move |_| delete_target.set(None)
                    on:wheel=move |e| e.stop_propagation()
                >
                    <div
                        class="board-modal vault-modal vault-modal-confirm"
                        role="dialog"
                        aria-modal="true"
                        aria-labelledby="vault-delete-title"
                        on:click=move |e| e.stop_propagation()
                    >
                        <header class="board-modal-head">
                            <div>
                                <h2 id="vault-delete-title">"Delete secret?"</h2>
                                <p>
                                    "This cannot be undone. Resources using "
                                    <strong class="mono-value">{move || delete_target.get().map(|(_, k)| k).unwrap_or_default()}</strong>
                                    " will fail until reconfigured."
                                </p>
                            </div>
                            <button type="button" class="board-modal-close" on:click=move |_| delete_target.set(None)>"Close"</button>
                        </header>
                        <div class="board-modal-body">
                            <div class="vault-modal-actions">
                                <button type="button" class="secondary-button" on:click=move |_| delete_target.set(None)>"Cancel"</button>
                                <button
                                    type="button"
                                    class="primary-button vault-danger-button"
                                    disabled=move || delete_action.pending().get()
                                    on:click=move |_| {
                                        if let Some((id, _)) = delete_target.get_untracked() {
                                            delete_action.dispatch(DeleteDashboardSecret {
                                                org_slug: org_slug.get_untracked(),
                                                secret_id: id,
                                            });
                                        }
                                    }
                                >
                                    {move || if delete_action.pending().get() { "Deleting…" } else { "Delete secret" }}
                                </button>
                            </div>
                        </div>
                    </div>
                </div>
            </Show>
        </div>
    }
}


#[island]
pub fn AccountProfileCard() -> impl IntoView {
    let profile = browser_load(get_account_profile);
    let action = ServerAction::<UpdateAccountProfile>::new();
    let pending = action.pending();
    let value = action.value();

    let (first_name, set_first_name) = signal(String::new());
    let (last_name, set_last_name) = signal(String::new());
    let (display_name, set_display_name) = signal(String::new());
    let (username, set_username) = signal(String::new());
    let (is_public, set_is_public) = signal(false);
    let (avatar_data_url, set_avatar_data_url) = signal(None::<String>);
    let (avatar_dirty, set_avatar_dirty) = signal(false);
    let (client_error, set_client_error) = signal(None::<String>);
    let (seeded, set_seeded) = signal(false);

    Effect::new(move |_| {
        if seeded.get() {
            return;
        }
        // Prefer a successful save result so the form stays in sync after update.
        if let Some(Ok(saved)) = value.get() {
            seed_profile_form(
                &saved,
                set_first_name,
                set_last_name,
                set_display_name,
                set_username,
                set_is_public,
                set_avatar_data_url,
            );
            set_avatar_dirty.set(false);
            set_seeded.set(true);
            return;
        }
        if let Some(Ok(loaded)) = profile.get() {
            seed_profile_form(
                &loaded,
                set_first_name,
                set_last_name,
                set_display_name,
                set_username,
                set_is_public,
                set_avatar_data_url,
            );
            set_avatar_dirty.set(false);
            set_seeded.set(true);
        }
    });

    // After a successful save, re-seed from the response.
    Effect::new(move |_| {
        if let Some(Ok(saved)) = value.get() {
            seed_profile_form(
                &saved,
                set_first_name,
                set_last_name,
                set_display_name,
                set_username,
                set_is_public,
                set_avatar_data_url,
            );
            set_avatar_dirty.set(false);
            set_client_error.set(None);
        }
    });

    let preview_initials = move || {
        profile_initials(
            &display_name.get(),
            &first_name.get(),
            &last_name.get(),
            profile
                .get()
                .and_then(Result::ok)
                .and_then(|p| p.email)
                .as_deref()
                .unwrap_or(""),
        )
    };

    let on_avatar_file = move |event| {
        #[cfg(feature = "hydrate")]
        {
            use wasm_bindgen::JsCast;
            let input: web_sys::HtmlInputElement = event_target(&event);
            spawn_local(async move {
                match pick_image_data_url(input, 250_000).await {
                    Ok(value) if value.is_null() || value.is_undefined() => {}
                    Ok(value) => {
                        if let Some(data_url) = value.as_string() {
                            set_avatar_data_url.set(Some(data_url));
                            set_avatar_dirty.set(true);
                            set_client_error.set(None);
                        }
                    }
                    Err(error) => {
                        let message = error
                            .as_string()
                            .unwrap_or_else(|| "Could not read image.".to_owned());
                        set_client_error.set(Some(message));
                    }
                }
            });
        }
        #[cfg(not(feature = "hydrate"))]
        {
            let _ = event;
        }
    };

    view! {
        <section class="panel profile-editor">
            {move || match profile.get() {
                Some(Err(error)) => view! {
                    <p class="error-banner">{server_error_text(error)}</p>
                }.into_any(),
                None if !seeded.get() => view! {
                    <div class="profile-loading" aria-busy="true">
                        <div class="profile-skeleton-avatar"></div>
                        <div class="profile-skeleton-lines">
                            <span></span><span></span><span></span>
                        </div>
                    </div>
                }.into_any(),
                _ => view! {
                    <div class="profile-editor-body">
                        // Centered identity: avatar + one primary line + optional handle
                        <header class="profile-identity-strip">
                            <div
                                class="profile-avatar-wrap"
                                class:has-photo=move || {
                                    avatar_data_url
                                        .get()
                                        .as_ref()
                                        .is_some_and(|url| !url.is_empty())
                                }
                            >
                                <Show when=move || {
                                    avatar_data_url
                                        .get()
                                        .as_ref()
                                        .is_some_and(|url| !url.is_empty())
                                }>
                                    <button
                                        type="button"
                                        class="profile-avatar-clear"
                                        aria-label="Remove photo"
                                        title="Remove photo"
                                        on:click=move |ev| {
                                            ev.prevent_default();
                                            ev.stop_propagation();
                                            set_avatar_data_url.set(None);
                                            set_avatar_dirty.set(true);
                                            set_client_error.set(None);
                                        }
                                    >
                                        <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
                                            <path
                                                fill="currentColor"
                                                d="M3.72 3.72a.75.75 0 0 1 1.06 0L8 6.94l3.22-3.22a.75.75 0 1 1 1.06 1.06L9.06 8l3.22 3.22a.75.75 0 1 1-1.06 1.06L8 9.06l-3.22 3.22a.75.75 0 0 1-1.06-1.06L6.94 8 3.72 4.78a.75.75 0 0 1 0-1.06Z"
                                            />
                                        </svg>
                                    </button>
                                </Show>
                                <label class="profile-avatar-control" title="Change photo">
                                    <input
                                        type="file"
                                        accept="image/png,image/jpeg,image/webp,image/gif"
                                        class="profile-file-input"
                                        aria-label="Upload profile photo"
                                        on:change=on_avatar_file
                                    />
                                    <span class="profile-avatar-disk" aria-hidden="true">
                                        {move || match avatar_data_url.get() {
                                            Some(url) if !url.is_empty() => view! {
                                                <img class="profile-avatar-img" src=url alt="" />
                                            }.into_any(),
                                            _ => view! {
                                                <span class="profile-avatar-fallback">{preview_initials()}</span>
                                            }.into_any(),
                                        }}
                                        <span class="profile-avatar-veil">
                                            <svg class="profile-avatar-camera" viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
                                                <path
                                                    fill="currentColor"
                                                    d="M9 3.75A1.75 1.75 0 0 1 10.53 2.5h2.94A1.75 1.75 0 0 1 15 3.75V5h2.25A2.75 2.75 0 0 1 20 7.75v9.5A2.75 2.75 0 0 1 17.25 20H6.75A2.75 2.75 0 0 1 4 17.25v-9.5A2.75 2.75 0 0 1 6.75 5H9V3.75Zm1.5 1.5V5h3V5.25h-3ZM12 9a4 4 0 1 0 0 8 4 4 0 0 0 0-8Zm0 1.5a2.5 2.5 0 1 1 0 5 2.5 2.5 0 0 1 0-5Z"
                                                />
                                            </svg>
                                        </span>
                                    </span>
                                </label>
                            </div>
                            <div class="profile-identity-copy">
                                <h2 class="profile-display-preview">
                                    {move || {
                                        let display = display_name.get();
                                        let first = first_name.get();
                                        let last = last_name.get();
                                        let composed = format!("{first} {last}").trim().to_owned();
                                        let email = profile
                                            .get()
                                            .and_then(Result::ok)
                                            .and_then(|p| p.email)
                                            .unwrap_or_default();
                                        if !display.trim().is_empty() {
                                            display
                                        } else if !composed.is_empty() {
                                            composed
                                        } else if !email.is_empty() {
                                            email
                                        } else {
                                            "Your name".to_owned()
                                        }
                                    }}
                                </h2>
                                // Handle only when set — never show a placeholder @handle.
                                <Show when=move || !username.get().trim().is_empty()>
                                    <p class="profile-handle-preview">
                                        {move || format!("@{}", username.get().trim().to_ascii_lowercase())}
                                    </p>
                                </Show>
                                // Email only when primary title is a name (avoid duplicate email lines).
                                <Show when=move || {
                                    let display = display_name.get();
                                    let first = first_name.get();
                                    let last = last_name.get();
                                    let composed = format!("{first} {last}").trim().to_owned();
                                    let has_name = !display.trim().is_empty() || !composed.is_empty();
                                    has_name && profile.get().and_then(Result::ok).and_then(|p| p.email).is_some()
                                }>
                                    <p class="profile-email-line">
                                        {move || profile
                                            .get()
                                            .and_then(Result::ok)
                                            .and_then(|p| p.email)
                                            .unwrap_or_default()}
                                    </p>
                                </Show>
                            </div>
                        </header>

                        <div class="profile-sections">
                            <section class="profile-section">
                                <div class="profile-section-head">
                                    <h3>"Name"</h3>
                                    <p>"Legal name stays private unless you publish your profile."</p>
                                </div>
                                <div class="auth-fields profile-form-grid">
                                    <label class="auth-field">
                                        <span>"First name"</span>
                                        <input
                                            class="auth-input"
                                            type="text"
                                            autocomplete="given-name"
                                            maxlength="60"
                                            prop:value=move || first_name.get()
                                            on:input=move |event| {
                                                set_client_error.set(None);
                                                set_first_name.set(event_target_value(&event));
                                            }
                                        />
                                    </label>
                                    <label class="auth-field">
                                        <span>"Last name"</span>
                                        <input
                                            class="auth-input"
                                            type="text"
                                            autocomplete="family-name"
                                            maxlength="60"
                                            prop:value=move || last_name.get()
                                            on:input=move |event| {
                                                set_client_error.set(None);
                                                set_last_name.set(event_target_value(&event));
                                            }
                                        />
                                    </label>
                                    <label class="auth-field profile-field-span">
                                        <span>"Display name"</span>
                                        <input
                                            class="auth-input"
                                            type="text"
                                            autocomplete="nickname"
                                            maxlength="80"
                                            prop:value=move || display_name.get()
                                            on:input=move |event| {
                                                set_client_error.set(None);
                                                set_display_name.set(event_target_value(&event));
                                            }
                                        />
                                        <small>"Shown publicly. Falls back to first + last when empty."</small>
                                    </label>
                                </div>
                            </section>

                            <section class="profile-section">
                                <div class="profile-section-head">
                                    <h3>"Handle"</h3>
                                    <p>"Your unique @username. Required for a public profile link."</p>
                                </div>
                                <div class="auth-fields">
                                    <label class="auth-field">
                                        <span>"Username"</span>
                                        <div class="profile-username-field">
                                            <span class="profile-username-at" aria-hidden="true">"@"</span>
                                            <input
                                                class="auth-input profile-username-input"
                                                type="text"
                                                autocomplete="username"
                                                spellcheck="false"
                                                maxlength="30"
                                                prop:value=move || username.get()
                                                on:input=move |event| {
                                                    set_client_error.set(None);
                                                    let raw = event_target_value(&event);
                                                    let cleaned = raw
                                                        .chars()
                                                        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
                                                        .collect::<String>()
                                                        .to_ascii_lowercase();
                                                    set_username.set(cleaned);
                                                }
                                            />
                                        </div>
                                        <small>"3–30 characters · letters, numbers, underscore"</small>
                                    </label>
                                </div>
                            </section>

                            <section class="profile-section profile-section-privacy">
                                <div class="profile-section-head">
                                    <h3>"Visibility"</h3>
                                    <p>"Profiles are private until you choose to publish."</p>
                                </div>
                                <label class="profile-switch">
                                    <input
                                        type="checkbox"
                                        role="switch"
                                        prop:checked=move || is_public.get()
                                        on:change=move |event| {
                                            set_client_error.set(None);
                                            set_is_public.set(event_target_checked(&event));
                                        }
                                    />
                                    <span class="profile-switch-track" aria-hidden="true">
                                        <span class="profile-switch-thumb"></span>
                                    </span>
                                    <span class="profile-switch-copy">
                                        <strong>"Public profile"</strong>
                                        <small>
                                            {move || if is_public.get() {
                                                "Anyone with your link can see your name, @handle, and photo."
                                            } else {
                                                "Only you can see this profile."
                                            }}
                                        </small>
                                    </span>
                                </label>
                                <Show when=move || {
                                    is_public.get() && !username.get().trim().is_empty()
                                }>
                                    <p class="profile-public-link">
                                        <span class="profile-public-link-label">"Live at"</span>
                                        <a
                                            class="profile-public-link-url"
                                            href=move || format!(
                                                "/u/{}",
                                                username.get().trim().to_ascii_lowercase()
                                            )
                                        >
                                            {move || format!(
                                                "/u/{}",
                                                username.get().trim().to_ascii_lowercase()
                                            )}
                                        </a>
                                    </p>
                                </Show>
                            </section>
                        </div>

                        <footer class="profile-footer">
                            <button
                                type="button"
                                class="primary-button"
                                disabled=move || pending.get()
                                on:click=move |_| {
                                    set_client_error.set(None);
                                    let handle = username.get_untracked().trim().to_owned();
                                    if !handle.is_empty()
                                        && (handle.len() < 3 || handle.len() > 30)
                                    {
                                        set_client_error.set(Some(
                                            "Username must be 3–30 characters.".to_owned(),
                                        ));
                                        return;
                                    }
                                    action.dispatch(UpdateAccountProfile {
                                        first_name: first_name.get_untracked(),
                                        last_name: last_name.get_untracked(),
                                        display_name: display_name.get_untracked(),
                                        username: handle,
                                        is_public: is_public.get_untracked(),
                                        avatar_data_url: if avatar_dirty.get_untracked() {
                                            Some(
                                                avatar_data_url
                                                    .get_untracked()
                                                    .unwrap_or_default(),
                                            )
                                        } else {
                                            None
                                        },
                                    });
                                }
                            >
                                {move || if pending.get() { "Saving…" } else { "Save changes" }}
                            </button>
                            <p class="error-banner" hidden=move || client_error.get().is_none()>
                                {move || client_error.get().unwrap_or_default()}
                            </p>
                            <Show when=move || {
                                value.get().is_some_and(|result| result.is_err())
                            }>
                                <p class="error-banner">
                                    {move || action_result_text(value.get())}
                                </p>
                            </Show>
                            <Show when=move || matches!(value.get(), Some(Ok(_)))>
                                <p class="auth-success profile-save-ok">
                                    <span>"Saved"</span>
                                </p>
                            </Show>
                        </footer>
                    </div>
                }.into_any(),
            }}
        </section>
    }
}


pub fn seed_profile_form(
    profile: &ProfileView,
    set_first_name: WriteSignal<String>,
    set_last_name: WriteSignal<String>,
    set_display_name: WriteSignal<String>,
    set_username: WriteSignal<String>,
    set_is_public: WriteSignal<bool>,
    set_avatar_data_url: WriteSignal<Option<String>>,
) {
    set_first_name.set(profile.first_name.clone());
    set_last_name.set(profile.last_name.clone());
    set_display_name.set(profile.display_name.clone());
    set_username.set(profile.username.clone());
    set_is_public.set(profile.is_public);
    set_avatar_data_url.set(profile.avatar_data_url.clone());
}


pub fn profile_initials(display_name: &str, first: &str, last: &str, email: &str) -> String {
    let display = display_name.trim();
    if !display.is_empty() {
        let parts: Vec<&str> = display.split_whitespace().collect();
        if parts.len() >= 2 {
            let a = parts[0].chars().next().unwrap_or('?');
            let b = parts[1].chars().next().unwrap_or('?');
            return format!(
                "{}{}",
                a.to_ascii_uppercase(),
                b.to_ascii_uppercase()
            );
        }
        return display
            .chars()
            .take(2)
            .map(|c| c.to_ascii_uppercase())
            .collect();
    }
    let first = first.trim();
    let last = last.trim();
    match (first.chars().next(), last.chars().next()) {
        (Some(a), Some(b)) => format!("{}{}", a.to_ascii_uppercase(), b.to_ascii_uppercase()),
        (Some(a), None) => a.to_ascii_uppercase().to_string(),
        (None, Some(b)) => b.to_ascii_uppercase().to_string(),
        _ => account_initials(email),
    }
}


pub fn account_initials(email: &str) -> String {
    let local = email.split('@').next().unwrap_or(email).trim();
    let mut chars = local.chars().filter(|c| c.is_alphanumeric());
    match (chars.next(), chars.next()) {
        (Some(a), Some(b)) => format!("{}{}", a.to_ascii_uppercase(), b.to_ascii_uppercase()),
        (Some(a), None) => a.to_ascii_uppercase().to_string(),
        _ => "?".to_string(),
    }
}


#[island]
pub fn PublicProfileCard(handle: String) -> impl IntoView {
    let profile = {
        let handle = handle.clone();
        browser_load(move || get_public_profile(handle))
    };

    view! {
        <section class="panel public-profile-panel">
            {move || match profile.get() {
                None => view! { <p class="result-line">"Loading profile…"</p> }.into_any(),
                Some(Err(_)) => view! {
                    <div class="public-profile-empty">
                        <div class="profile-avatar-fallback public-profile-empty-avatar" aria-hidden="true">"?"</div>
                        <h2>"Profile unavailable"</h2>
                        <p class="result-line">
                            "This @handle is private or does not exist."
                        </p>
                        <a class="link-button" href="/">"Back home"</a>
                    </div>
                }.into_any(),
                Some(Ok(view)) => {
                    let display = if !view.display_name.trim().is_empty() {
                        view.display_name.clone()
                    } else {
                        let composed = format!("{} {}", view.first_name, view.last_name)
                            .trim()
                            .to_owned();
                        if composed.is_empty() {
                            format!("@{}", view.username)
                        } else {
                            composed
                        }
                    };
                    let initials = profile_initials(
                        &view.display_name,
                        &view.first_name,
                        &view.last_name,
                        &view.username,
                    );
                    let handle_label = format!("@{}", view.username);
                    let avatar = view.avatar_data_url.clone();
                    let legal_name = {
                        let composed = format!("{} {}", view.first_name, view.last_name)
                            .trim()
                            .to_owned();
                        if composed.is_empty() || composed == display {
                            None
                        } else {
                            Some(composed)
                        }
                    };
                    view! {
                        <div class="public-profile-hero">
                            <div class="public-profile-avatar" aria-hidden="true">
                                {match avatar {
                                    Some(url) if !url.is_empty() => view! {
                                        <img class="profile-avatar-img" src=url alt="" />
                                    }.into_any(),
                                    _ => view! {
                                        <span class="profile-avatar-fallback">{initials}</span>
                                    }.into_any(),
                                }}
                            </div>
                            <div class="public-profile-meta">
                                <p class="profile-kicker">"Public profile"</p>
                                <h2>{display}</h2>
                                <p class="profile-handle-preview">{handle_label}</p>
                                {legal_name.map(|name| view! {
                                    <p class="profile-email-line">{name}</p>
                                })}
                            </div>
                        </div>
                    }.into_any()
                }
            }}
        </section>
    }
}


#[island]
pub fn ChangePasswordForm() -> impl IntoView {
    let action = ServerAction::<ChangePassword>::new();
    let pending = action.pending();
    let value = action.value();
    let (current_password, set_current_password) = signal(String::new());
    let (new_password, set_new_password) = signal(String::new());
    let (confirm_password, set_confirm_password) = signal(String::new());
    let (client_error, set_client_error) = signal(None::<String>);

    let can_submit = move || {
        let current = current_password.get();
        let next = new_password.get();
        let confirm = confirm_password.get();
        !pending.get()
            && !current.is_empty()
            && next.chars().count() >= 15
            && next == confirm
            && next != current
    };

    let disabled = Signal::derive(move || !can_submit());
    let success_msg = Signal::derive(move || {
        if matches!(value.get(), Some(Ok(_))) {
            Some("Password updated. Other sessions were signed out.".to_owned())
        } else {
            None
        }
    });

    view! {
        <Panel class="password-change-panel".to_owned()>
            <div class="session-panel-head">
                <div>
                    <SectionLabel>"Credential"</SectionLabel>
                    <h2>"Change password"</h2>
                </div>
            </div>
            <p class="passkey-lede">
                "Enter your current password to confirm it's you. Use at least 15 characters for the new password. Other signed-in sessions will be signed out."
            </p>
            <FieldGroup>
                <Field label="Current password">
                    <TextInput
                        input_type="password"
                        autocomplete="current-password"
                        value=current_password
                        on_input=Callback::new(move |v: String| {
                            set_client_error.set(None);
                            set_current_password.set(v);
                        })
                    />
                </Field>
                <Field label="New password" hint="Minimum 15 characters. Prefer a long phrase.">
                    <TextInput
                        input_type="password"
                        autocomplete="new-password"
                        value=new_password
                        on_input=Callback::new(move |v: String| {
                            set_client_error.set(None);
                            set_new_password.set(v);
                        })
                    />
                </Field>
                <Field label="Confirm new password">
                    <TextInput
                        input_type="password"
                        autocomplete="new-password"
                        value=confirm_password
                        on_input=Callback::new(move |v: String| {
                            set_client_error.set(None);
                            set_confirm_password.set(v);
                        })
                    />
                </Field>
            </FieldGroup>
            <div class="account-card-actions">
                <PrimaryButton
                    disabled=disabled
                    on_click=Callback::new(move |_| {
                        let current = current_password.get_untracked();
                        let next = new_password.get_untracked();
                        let confirm = confirm_password.get_untracked();
                        if next.chars().count() < 15 {
                            set_client_error.set(Some("New password must be at least 15 characters.".to_owned()));
                            return;
                        }
                        if next != confirm {
                            set_client_error.set(Some("New password and confirmation do not match.".to_owned()));
                            return;
                        }
                        if next == current {
                            set_client_error.set(Some("New password must be different from the current password.".to_owned()));
                            return;
                        }
                        set_client_error.set(None);
                        action.dispatch(ChangePassword {
                            current_password: current,
                            new_password: next,
                        });
                    })
                >
                    {move || if pending.get() { "Updating password…" } else { "Update password" }}
                </PrimaryButton>
                <ErrorBanner message=client_error />
                <Show when=move || value.get().is_some()>
                    <p class="result-line">{move || action_result_text(value.get())}</p>
                </Show>
                <SuccessBanner message=success_msg />
                <a class="auth-text-link" href="/forgot-password">"Forgot password? Use email reset"</a>
            </div>
        </Panel>
    }
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
        <section class="panel">
            <div class="session-panel-head">
                <div>
                    <p class="section-label">"Devices"</p>
                    <h2>"Active sessions"</h2>
                </div>
            </div>
            <p class="passkey-lede">
                "Revoking ends access for that browser or device. Signing out this browser leaves the page immediately."
            </p>
            <div class="client-data-slot">
                {move || match sessions.get() {
                    Some(Ok(_)) => {
                        let list = rows.get();
                        if list.is_empty() {
                            view! { <p class="result-line">"No active sessions"</p> }.into_any()
                        } else {
                            view! {
                                <div class="session-list">
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
                                                    "compact-panel session-card session-card-current"
                                                } else {
                                                    "compact-panel session-card"
                                                }>
                                                    <div class="session-card-head">
                                                        <h3>{if is_current { "This browser" } else { "Other device" }}</h3>
                                                        <span class="session-assurance">{assurance.to_uppercase()}</span>
                                                    </div>
                                                    <p class="result-line">
                                                        {format!("Expires at {expires}")}
                                                    </p>
                                                    <button
                                                        type="button"
                                                        class=if is_current { "primary-button" } else { "secondary-button" }
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
                    Some(Err(error)) => view! { <p class="error-banner">{server_error_text(error)}</p> }.into_any(),
                    None => view! { <p class="result-line">"Loading sessions"</p> }.into_any(),
                }}
            </div>
            <p class="auth-success" hidden=move || status_message.get().is_none() || error_message.get().is_some()>
                {move || status_message.get().unwrap_or_default()}
            </p>
            <p class="error-banner" hidden=move || error_message.get().is_none()>
                {move || error_message.get().unwrap_or_default()}
            </p>
        </section>
    }
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
        <div class="passkey-flow">
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
                        <div class="passkey-focus-wrap">
                            <section class="panel passkey-ceremony-panel">
                                <div class="passkey-wizard-progress" aria-hidden="true">
                                    <span class="passkey-wizard-step is-done">"1"</span>
                                    <span class="passkey-wizard-line is-done"></span>
                                    <span class="passkey-wizard-step is-active">"2"</span>
                                    <span class="passkey-wizard-line"></span>
                                    <span class="passkey-wizard-step">"3"</span>
                                </div>
                                <p class="section-label">"Creating passkey"</p>
                                <h2>
                                    {move || if ceremony_cancelled {
                                        "Passkey not created"
                                    } else if ceremony_failed {
                                        "Could not create passkey"
                                    } else {
                                        "Confirm with your device"
                                    }}
                                </h2>
                                <p class="passkey-lede">
                                    {move || if ceremony_cancelled {
                                        "You closed the browser prompt. Try again when you're ready, or use a different device / security key."
                                    } else if ceremony_failed {
                                        "You can retry the browser prompt, or go back and try another device / security key."
                                    } else {
                                        "Use Face ID, Touch ID, Windows Hello, a phone QR passkey, or a security key. Keep this tab open until the prompt finishes."
                                    }}
                                </p>
                                <div class="passkey-device-card" aria-hidden="true" hidden=move || ceremony_failed>
                                    <div class="passkey-device-icon">
                                        <span></span>
                                        <span></span>
                                    </div>
                                    <p>"Waiting for authenticator…"</p>
                                </div>
                                <p class="result-line" hidden=move || ceremony_failed>
                                    {move || if verify_pending.get() {
                                        "Saving passkey to your account…"
                                    } else if start_pending.get() {
                                        "Starting secure registration…"
                                    } else {
                                        "Follow the prompt on your device"
                                    }}
                                </p>
                                <p
                                    class="error-banner"
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
                                <p class="error-banner" hidden=move || !matches!(start_value.get(), Some(Err(_)))>
                                    {move || match start_value.get() {
                                        Some(Err(error)) => server_error_text(error),
                                        _ => String::new(),
                                    }}
                                </p>
                                <p class="error-banner" hidden=move || !matches!(verify_value.get(), Some(Err(_)))>
                                    {move || match verify_value.get() {
                                        Some(Err(error)) => server_error_text(error),
                                        _ => String::new(),
                                    }}
                                </p>
                                <div class="button-row">
                                    <button
                                        type="button"
                                        class="primary-button"
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
                                        class="secondary-button"
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
                        <div class="passkey-focus-wrap">
                            <section class="panel passkey-success-panel">
                                <span class="mfa-badge mfa-badge-on">"Passkey registered"</span>
                                <h2>"You can sign in without a password"</h2>
                                <p class="passkey-lede">
                                    "Next time, choose passkey on the sign-in page and approve with this device. Your session assurance is elevated for phishing-resistant sign-in."
                                </p>
                                <div class="button-row">
                                    <a class="primary-button" href="/account/sessions">"Review sessions"</a>
                                    <a class="secondary-button" href="/account/profile">"Back to profile"</a>
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
                    <div class="passkey-overview">
                        <section class="panel passkey-status-panel">
                            <div class="mfa-status-head">
                                <div>
                                    <p class="section-label">"Phishing-resistant sign-in"</p>
                                    <h2>"Passkeys"</h2>
                                    <p class="passkey-lede">
                                        "A passkey lets you sign in with the biometrics or PIN already on this device. It cannot be phished like a password."
                                    </p>
                                </div>
                                {if badge_on {
                                    view! { <span class="mfa-badge mfa-badge-on">{badge}</span> }.into_any()
                                } else {
                                    view! { <span class="mfa-badge mfa-badge-off">{badge}</span> }.into_any()
                                }}
                            </div>
                            <dl class="kv mfa-status-kv">
                                <dt>"Account"</dt>
                                <dd>{session_email.clone()}</dd>
                                <dt>"Deployment"</dt>
                                <dd>{deployment_label}</dd>
                                <dt>"This browser"</dt>
                                <dd>{if device_ok { "Supports WebAuthn" } else { "No passkey API" }}</dd>
                            </dl>
                        </section>

                        {if !caps_loaded {
                            view! {
                                <section class="panel">
                                    <p class="result-line">"Loading passkey settings…"</p>
                                </section>
                            }.into_any()
                        } else if !passkeys_on {
                            view! {
                                <section class="panel">
                                    <p class="section-label">"Operator note"</p>
                                    <h2>"Passkeys are off for this deployment"</h2>
                                    <p class="passkey-lede">
                                        "Turn them on with AUTH_ENABLE_PASSKEYS=true, then set AUTH_PASSKEY_RP_ID and AUTH_PASSKEY_ORIGIN to match your public site origin (for local: localhost and http://localhost:3008 — not 127.0.0.1)."
                                    </p>
                                    <ol class="mfa-steps-preview">
                                        <li><strong>"RP ID"</strong>" must match the site host (no port). Use localhost, never an IP address."</li>
                                        <li><strong>"Origin"</strong>" must match the exact browser origin including scheme and port."</li>
                                        <li><strong>"HTTPS"</strong>" is required outside localhost."</li>
                                    </ol>
                                    <div class="button-row">
                                        <a class="secondary-button" href="/account/mfa">"Use authenticator app instead"</a>
                                        <a class="secondary-button" href="/account/password">"Password settings"</a>
                                    </div>
                                </section>
                            }.into_any()
                        } else if !device_ok {
                            view! {
                                <section class="panel">
                                    <p class="section-label">"Device"</p>
                                    <h2>"This browser cannot create passkeys"</h2>
                                    <p class="passkey-lede">
                                        "Try a current Chrome, Safari, Edge, or Firefox build, or open this page on a phone that supports platform authenticators."
                                    </p>
                                    <div class="button-row">
                                        <a class="secondary-button" href="/account/mfa">"Set up authenticator app"</a>
                                        <a class="secondary-button" href="/login">"Password sign-in"</a>
                                    </div>
                                </section>
                            }.into_any()
                        } else {
                            view! {
                                <section class="panel">
                                    <p class="section-label">"Add to this account"</p>
                                    <h2>"Create a passkey"</h2>
                                    <ol class="mfa-steps-preview">
                                        <li><strong>"Start"</strong>" registration for "{session_email.clone()}"."</li>
                                        <li><strong>"Approve"</strong>" the system prompt (biometrics or security key)."</li>
                                        <li><strong>"Done"</strong>" — use passkey next time you sign in."</li>
                                    </ol>
                                    <button
                                        type="button"
                                        class="primary-button"
                                        disabled=move || start_pending.get() || verify_pending.get() || !can_register
                                        on:click=move |_| {
                                            set_client_error.set(None);
                                            start_action.dispatch(StartPasskeyRegistration {
                                                email: email_dispatch.clone(),
                                                redirect_url: Some("/account/passkeys".to_owned()),
                                            });
                                        }
                                    >"Create a passkey"</button>
                                    <p class="passkey-hint">
                                        "Works with iCloud Keychain, Google Password Manager, 1Password, and hardware keys (YubiKey, etc.)."
                                    </p>
                                    <p class="error-banner" hidden=move || client_error.get().is_none()>
                                        {move || client_error.get().unwrap_or_default()}
                                    </p>
                                    <p class="error-banner" hidden=move || !matches!(start_value.get(), Some(Err(_)))>
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



/// Best-effort async sleep for hydrate (vault reveal auto-mask).
#[cfg(feature = "hydrate")]
async fn gloo_timers_sleep_ms(ms: u64) {
    use wasm_bindgen_futures::JsFuture;
    use js_sys::Promise;
    let promise = Promise::new(&mut |resolve, _reject| {
        if let Some(window) = web_sys::window() {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                &resolve,
                ms as i32,
            );
        } else {
            let _ = resolve.call0(&wasm_bindgen::JsValue::NULL);
        }
    });
    let _ = JsFuture::from(promise).await;
}

#[cfg(not(feature = "hydrate"))]
async fn gloo_timers_sleep_ms(_ms: u64) {}
