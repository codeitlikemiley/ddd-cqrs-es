//! Auth forms and interactive islands (login, OAuth, logout).

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use crate::app::account::PasskeyManager;
use crate::app::helpers::{
    action_result_text, first_http_url_in_text, is_passkey_cancel_message, next_url,
    one_time_token_from_url, percent_encode_component, redirect_browser, selected_action_error,
    selected_auth_error, server_error_text, validate_email_only, validate_login_form,
};
#[cfg(feature = "hydrate")]
use crate::app::helpers::{passkey_js_error, passkey_js_string};
use crate::app::{
    AcceptOrganizationInvitation, CompleteEmailVerification, CompleteOauthCallback,
    CompletePasswordReset, LatestDevelopmentMail, LoginEmailPassword, LogoutCurrentSession,
    RegisterEmailPassword, ResendEmailVerification, StartOauthLogin, StartPasskeyLogin,
    StartPasswordReset, VerifyPasskeyLogin, browser_load, complete_email_verification,
    complete_oauth_callback, complete_password_reset, development_mail_capture_enabled,
    get_auth_capabilities, get_current_session, latest_development_mail, list_auth_providers,
    login_email_password, logout_current_session, register_email_password,
    resend_email_verification, start_oauth_login, start_passkey_login, start_password_reset,
    verify_passkey_login,
};
use crate::contracts::{
    AuthCapabilities, AuthProviderSummary, CapturedMailResponse, LoginCompletionResponse,
    OrganizationSummary, PasswordResetStartResponse, SessionView,
};
use crate::ui::classes::{
    AUTH_ALT_BUTTON, AUTH_ALT_STACK, AUTH_BUTTON_SPINNER, AUTH_COPY, AUTH_DIVIDER, AUTH_FORM,
    AUTH_INLINE_ERROR, AUTH_MODE_BUTTON, AUTH_MODE_BUTTON_ACTIVE, AUTH_MODE_SWITCH, AUTH_SECONDARY,
    AUTH_TEXT_LINK, AUTH_TITLE, BANNER_ERROR, BANNER_SUCCESS, BTN_AUTH_SUBMIT, BTN_PRIMARY,
    BTN_SECONDARY, BUTTON_ROW, FIELD, FIELD_GROUP, INPUT, PANEL, PANEL_COMPACT, RESULT_LINE,
    SECTION_LABEL,
};
use crate::ui::error_page_shell;
use leptos::prelude::*;
#[cfg(feature = "hydrate")]
use leptos::task::spawn_local;
use leptos_meta::*;
use leptos_router::hooks::use_params_map;
#[cfg(feature = "hydrate")]
use wasm_bindgen::prelude::*;
#[cfg(feature = "hydrate")]
use web_sys::window;

#[cfg(feature = "hydrate")]
use crate::app::{
    create_passkey_credential, get_passkey_credential, is_conditional_mediation_available,
    passkey_supported,
};

#[island]
pub fn EmailPasswordAuthForm(register_default: bool) -> impl IntoView {
    let login_action = ServerAction::<LoginEmailPassword>::new();
    let register_action = ServerAction::<RegisterEmailPassword>::new();
    let login_pending = login_action.pending();
    let register_pending = register_action.pending();
    let login_value = login_action.value();
    let register_value = register_action.value();
    let (register_mode, set_register_mode) = signal(register_default);
    // Apple-style sign-in: email → Continue → password + Sign in.
    let (password_step, set_password_step) = signal(register_default);
    let (email, set_email) = signal(String::new());
    let (password, set_password) = signal(String::new());
    let (client_error, set_client_error) = signal(None::<String>);
    let capture_enabled = browser_load(development_mail_capture_enabled);
    let capture_action = ServerAction::<LatestDevelopmentMail>::new();
    let capture_pending = capture_action.pending();
    let capture_value = capture_action.value();
    let registration_complete = RwSignal::new(false);

    // Shared-email passkey (modal + conditional autofill on password step).
    let passkey_start = ServerAction::<StartPasskeyLogin>::new();
    let passkey_verify = ServerAction::<VerifyPasskeyLogin>::new();
    let passkey_start_pending = passkey_start.pending();
    let passkey_verify_pending = passkey_verify.pending();
    let passkey_start_value = passkey_start.value();
    let passkey_verify_value = passkey_verify.value();
    let (passkey_mediation, set_passkey_mediation) = signal("".to_string());
    let (conditional_armed, set_conditional_armed) = signal(false);
    let capabilities = browser_load(get_auth_capabilities);

    Effect::new(move |_| {
        if let Some(Ok(response)) = login_value.get() {
            redirect_browser(&response.redirect_url);
        }
    });
    Effect::new(move |_| {
        if register_value.get().is_some_and(|result| result.is_ok()) {
            registration_complete.set(true);
        }
    });
    Effect::new(move |_| {
        if let Some(Ok(message)) = capture_value.get() {
            if let Some(action_url) = message.action_url.as_deref().filter(|url| !url.is_empty()) {
                redirect_browser(action_url);
            } else if let Some(action_url) = first_http_url_in_text(&message.body_text) {
                redirect_browser(&action_url);
            }
        }
    });
    Effect::new(move |_| {
        match passkey_verify_value.get() {
            Some(Ok(response)) => redirect_browser(&response.redirect_url),
            // Single error channel: client_error only (avoid a second auth-error block).
            Some(Err(error)) if passkey_mediation.get_untracked() != "conditional" => {
                set_client_error.set(Some(server_error_text(error)));
            }
            _ => {}
        }
    });
    Effect::new(move |_| {
        let mediation = passkey_mediation.get_untracked();
        let silent = mediation == "conditional";
        match passkey_start_value.get() {
            Some(Ok(response)) => {
                #[cfg(feature = "hydrate")]
                {
                    if !passkey_supported() {
                        if !silent {
                            redirect_browser("/auth/passkey-unsupported");
                        }
                        return;
                    }
                    let verify = passkey_verify;
                    let set_client_error = set_client_error;
                    let challenge_id = response.challenge_id;
                    let options_json = response.public_key_options_json;
                    let redirect_url = Some(next_url());
                    spawn_local(async move {
                        match get_passkey_credential(options_json, mediation.clone()).await {
                            Ok(value) => match passkey_js_string(value) {
                                Ok(credential_json) => {
                                    verify.dispatch(VerifyPasskeyLogin {
                                        challenge_id,
                                        credential_json,
                                        redirect_url,
                                    });
                                }
                                Err(error) => {
                                    if !silent {
                                        set_client_error.set(Some(error));
                                    }
                                }
                            },
                            Err(error) => {
                                let message = passkey_js_error(error);
                                if silent
                                    || message.contains("PASSKEY_CONDITIONAL_IDLE")
                                    || message.contains("cancelled")
                                {
                                    return;
                                }
                                set_client_error.set(Some(message));
                            }
                        }
                    });
                }
                #[cfg(not(feature = "hydrate"))]
                {
                    let _ = (response, mediation, silent);
                }
            }
            // Conditional start failures must never surface as login form errors.
            Some(Err(_)) if silent => {}
            // Modal passkey start failed (e.g. no passkey for this email) — one banner only.
            Some(Err(error)) => {
                let text = server_error_text(error);
                // Server often uses a generic credential error for privacy; be clearer on passkey click.
                let text = if text.to_ascii_lowercase().contains("incorrect")
                    || text.to_ascii_lowercase().contains("invalid credential")
                    || text.to_ascii_lowercase().contains("not found")
                {
                    "No passkey is available for this email. Sign in with your password, then add a passkey in Account → Passkeys.".to_owned()
                } else {
                    text
                };
                set_client_error.set(Some(text));
            }
            None => {}
        }
    });

    // After email is confirmed, arm Conditional UI so Chrome/Safari can offer a passkey in autofill.
    // Failures are silent — accounts without passkeys are normal.
    Effect::new(move |_| {
        let passkeys_on = capabilities
            .get()
            .and_then(Result::ok)
            .is_some_and(|caps| caps.passkeys_enabled);
        let ready = !register_mode.get()
            && password_step.get()
            && passkeys_on
            && !conditional_armed.get()
            && validate_email_only(&email.get()).is_ok();
        if !ready {
            return;
        }
        set_conditional_armed.set(true);
        #[cfg(feature = "hydrate")]
        {
            let email_value = email.get_untracked().trim().to_string();
            let passkey_start = passkey_start;
            let set_passkey_mediation = set_passkey_mediation;
            spawn_local(async move {
                let available = is_conditional_mediation_available()
                    .await
                    .ok()
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false);
                if !available {
                    return;
                }
                set_passkey_mediation.set("conditional".to_owned());
                passkey_start.dispatch(StartPasskeyLogin {
                    email: Some(email_value),
                    redirect_url: Some(next_url()),
                });
            });
        }
    });

    let submit_credentials = move || {
        // Sign-in step 1: email only → reveal password + Sign in.
        if !register_mode.get_untracked() && !password_step.get_untracked() {
            let email_value = email.get_untracked().trim().to_string();
            if let Err(error) = validate_email_only(&email_value) {
                set_client_error.set(Some(error));
                return;
            }
            set_email.set(email_value);
            set_client_error.set(None);
            set_password_step.set(true);
            set_conditional_armed.set(false);
            return;
        }
        let email_value = email.get_untracked().trim().to_string();
        let password_value = password.get_untracked();
        if let Err(error) =
            validate_login_form(&email_value, &password_value, register_mode.get_untracked())
        {
            set_client_error.set(Some(error));
            return;
        }
        set_client_error.set(None);
        let redirect_url = Some(next_url());
        if register_mode.get_untracked() {
            register_action.dispatch(RegisterEmailPassword {
                email: email_value,
                password: password_value,
                redirect_url,
            });
        } else {
            login_action.dispatch(LoginEmailPassword {
                email: email_value,
                password: password_value,
                redirect_url,
            });
        }
    };

    let start_passkey_modal = move |_| {
        let email_value = email.get_untracked().trim().to_string();
        if let Err(error) = validate_email_only(&email_value) {
            set_client_error.set(Some(error));
            // Stay on email step so the shared field is obvious.
            set_password_step.set(false);
            return;
        }
        set_client_error.set(None);
        // Modal ceremony — surface errors if the user explicitly chose passkey.
        // Password is ignored for passkey sign-in; only the shared email is used.
        set_passkey_mediation.set(String::new());
        set_conditional_armed.set(true); // avoid a racing conditional start
        passkey_start.dispatch(StartPasskeyLogin {
            email: Some(email_value),
            redirect_url: Some(next_url()),
        });
    };

    view! {
        <section class=AUTH_FORM>
            <div>
                <p class=SECTION_LABEL>"Authentication"</p>
                <h1 class=AUTH_TITLE>
                    {move || if register_mode.get() { "Create your workspace" } else { "Welcome back" }}
                </h1>
                <p class=AUTH_COPY>
                    {move || if register_mode.get() {
                        "Set up a password-backed workspace session."
                    } else if password_step.get() {
                        "Enter your password to continue."
                    } else {
                        "Enter your email to continue."
                    }}
                </p>
            </div>

            <Show when=move || registration_complete.get()>
                <div class=BANNER_SUCCESS>
                    <p><strong>"Account created."</strong> " Check your inbox for the one-time verification link."</p>
                    <Show when=move || matches!(capture_enabled.get(), Some(Ok(true)))>
                        <p>"Capture mode does not send internet email. The local worker stores the message for this example."</p>
                        <button
                            type="button"
                            class=AUTH_SECONDARY
                            disabled=move || capture_pending.get()
                            on:click=move |_| {
                                capture_action.dispatch(LatestDevelopmentMail {
                                    recipient: email.get_untracked(),
                                    message_kind: "email-verification".to_owned(),
                                });
                            }
                        >
                            {move || if capture_pending.get() { "Looking for message" } else { "Open captured verification link" }}
                        </button>
                        <Show when=move || selected_action_error(capture_value.get()).is_some()>
                            <p class=AUTH_INLINE_ERROR>
                                {move || selected_action_error(capture_value.get()).unwrap_or_default()}
                            </p>
                        </Show>
                    </Show>
                </div>
            </Show>

            <div class=AUTH_MODE_SWITCH role="tablist" aria-label="Authentication mode" hidden=move || registration_complete.get()>
                <button
                    type="button"
                    class=move || if register_mode.get() {
                        AUTH_MODE_BUTTON
                    } else {
                        AUTH_MODE_BUTTON_ACTIVE
                    }
                    on:click=move |_| {
                        set_register_mode.set(false);
                        set_password_step.set(false);
                        set_password.set(String::new());
                        set_conditional_armed.set(false);
                        set_client_error.set(None);
                    }
                >
                    "Sign in"
                </button>
                <button
                    type="button"
                    class=move || if register_mode.get() {
                        AUTH_MODE_BUTTON_ACTIVE
                    } else {
                        AUTH_MODE_BUTTON
                    }
                    on:click=move |_| {
                        set_register_mode.set(true);
                        set_password_step.set(true);
                        set_conditional_armed.set(false);
                        set_client_error.set(None);
                    }
                >
                    "Create workspace"
                </button>
            </div>

            <form class=FIELD_GROUP hidden=move || registration_complete.get() on:submit=move |event| {
                event.prevent_default();
                submit_credentials();
            }>
                <label class=FIELD>
                    <span>"Email"</span>
                    <input
                        class=INPUT
                        type="email"
                        name="email"
                        // webauthn enables Conditional UI passkey rows in supporting browsers
                        autocomplete="username webauthn"
                        placeholder="name@company.com"
                        prop:value=move || email.get()
                        aria-invalid=move || client_error.get().is_some()
                        on:input=move |event| {
                            set_email.set(event_target_value(&event));
                            set_client_error.set(None);
                            // Changing email invalidates any armed conditional ceremony.
                            set_conditional_armed.set(false);
                        }
                    />
                </label>
                <label
                    class=FIELD
                    hidden=move || !register_mode.get() && !password_step.get()
                >
                    <span>"Password"</span>
                    <input
                        class=INPUT
                        type="password"
                        name="password"
                        autocomplete=move || if register_mode.get() { "new-password" } else { "current-password" }
                        placeholder="Enter your password"
                        prop:value=move || password.get()
                        aria-invalid=move || client_error.get().is_some()
                        on:input=move |event| {
                            set_password.set(event_target_value(&event));
                            set_client_error.set(None);
                        }
                    />
                    <small hidden=move || !register_mode.get()>
                        "Use 15 to 128 characters. Only a derived password hash is stored."
                    </small>
                </label>

                // One error banner only: client validation + passkey + password/register server errors.
                <p
                    class=BANNER_ERROR
                    hidden=move || {
                        client_error.get().is_none()
                            && selected_auth_error(
                                register_mode.get(),
                                login_value.get(),
                                register_value.get(),
                            )
                            .is_none()
                    }
                >
                    {move || {
                        client_error
                            .get()
                            .or_else(|| {
                                selected_auth_error(
                                    register_mode.get(),
                                    login_value.get(),
                                    register_value.get(),
                                )
                            })
                            .unwrap_or_default()
                    }}
                </p>

                <button
                    type="submit"
                    class=BTN_AUTH_SUBMIT
                    disabled=move || {
                        login_pending.get()
                            || register_pending.get()
                            || passkey_start_pending.get()
                            || passkey_verify_pending.get()
                    }
                    aria-busy=move || {
                        if login_pending.get() || register_pending.get() {
                            "true"
                        } else {
                            "false"
                        }
                    }
                >
                    <span
                        class=AUTH_BUTTON_SPINNER
                        aria-hidden="true"
                        hidden=move || !(login_pending.get() || register_pending.get())
                    ></span>
                    <span>
                        {move || if login_pending.get() || register_pending.get() {
                            if register_mode.get() { "Creating workspace" } else { "Signing in" }
                        } else if register_mode.get() {
                            "Create workspace"
                        } else if password_step.get() {
                            "Sign in"
                        } else {
                            "Continue"
                        }}
                    </span>
                </button>
                <a
                    class=AUTH_TEXT_LINK
                    href="/forgot-password"
                    hidden=move || register_mode.get() || !password_step.get()
                >
                    "Forgot password?"
                </a>
                <button
                    type="button"
                    class=AUTH_TEXT_LINK
                    hidden=move || register_mode.get() || !password_step.get()
                    on:click=move |_| {
                        set_password_step.set(false);
                        set_password.set(String::new());
                        set_conditional_armed.set(false);
                        set_client_error.set(None);
                    }
                >
                    "Use a different email"
                </button>
            </form>

            <div
                class=AUTH_ALT_STACK
                hidden=move || registration_complete.get()
                    || !capabilities.get().is_some_and(|result| {
                        result.is_ok_and(|caps| {
                            (caps.oauth_enabled && !caps.providers.is_empty())
                                || caps.passkeys_enabled
                        })
                    })
            >
                <div class=AUTH_DIVIDER aria-hidden="true">
                    <span>"or"</span>
                </div>
                <div
                    class=AUTH_ALT_STACK
                    hidden=move || !capabilities.get().is_some_and(|result| {
                        result.is_ok_and(|caps| caps.oauth_enabled && !caps.providers.is_empty())
                    })
                >
                    <OAuthProviderButtons />
                </div>
                <button
                    type="button"
                    class=AUTH_ALT_BUTTON
                    hidden=move || !capabilities.get().is_some_and(|result| {
                        result.is_ok_and(|caps| caps.passkeys_enabled)
                    })
                    disabled=move || {
                        passkey_start_pending.get()
                            || passkey_verify_pending.get()
                            || login_pending.get()
                            || register_pending.get()
                    }
                    on:click=start_passkey_modal
                >
                    {move || if passkey_start_pending.get() || passkey_verify_pending.get() {
                        "Waiting for passkey…"
                    } else {
                        "Sign in with Passkey"
                    }}
                </button>
            </div>

            <p class="mt-2 text-xs leading-normal text-tertiary">
                "Protected by server-side validation, httpOnly session cookies, and tenant-scoped authorization checks."
            </p>
        </section>
    }
}

#[island]
pub fn EmailVerificationForm() -> impl IntoView {
    let action = ServerAction::<CompleteEmailVerification>::new();
    let value = action.value();
    let pending = action.pending();
    let dispatched = RwSignal::new(false);

    Effect::new(move |_| {
        if !dispatched.get()
            && let Some(token) = one_time_token_from_url()
        {
            dispatched.set(true);
            action.dispatch(CompleteEmailVerification {
                token,
                redirect_url: Some("/dashboard".to_string()),
            });
        }
    });
    Effect::new(move |_| {
        if let Some(Ok(response)) = value.get() {
            redirect_browser(&response.redirect_url);
        }
    });

    view! {
        <section class=AUTH_FORM>
            <div>
                <p class=SECTION_LABEL>"Email verification"</p>
                <h1 class=AUTH_TITLE>"Verify your email"</h1>
                <p class=AUTH_COPY>"The one-time link is hashed at rest and can be used once."</p>
            </div>
            <Show when=move || pending.get()>
                <p class=RESULT_LINE>"Verifying email"</p>
            </Show>
            <Show when=move || selected_action_error(value.get()).is_some()>
                <p class=BANNER_ERROR>{move || selected_action_error(value.get()).unwrap_or_default()}</p>
            </Show>
            <Show when=move || one_time_token_from_url().is_none()>
                <p class=BANNER_SUCCESS>"Open this page from the one-time link in your verification message."</p>
            </Show>
            <a class=AUTH_TEXT_LINK href="/verify-email/resend">"Request another message"</a>
        </section>
    }
}

#[island]
pub fn ResendVerificationForm() -> impl IntoView {
    let action = ServerAction::<ResendEmailVerification>::new();
    let pending = action.pending();
    let value = action.value();
    let (email, set_email) = signal(String::new());
    let capture_enabled = browser_load(development_mail_capture_enabled);
    let capture_action = ServerAction::<LatestDevelopmentMail>::new();
    let capture_pending = capture_action.pending();
    let capture_value = capture_action.value();

    Effect::new(move |_| {
        if let Some(Ok(message)) = capture_value.get() {
            if let Some(action_url) = message.action_url.as_deref().filter(|url| !url.is_empty()) {
                redirect_browser(action_url);
            } else if let Some(action_url) = first_http_url_in_text(&message.body_text) {
                redirect_browser(&action_url);
            }
        }
    });

    view! {
        <section class=AUTH_FORM>
            <div>
                <p class=SECTION_LABEL>"Email verification"</p>
                <h1 class=AUTH_TITLE>"Send a fresh link"</h1>
                <p class=AUTH_COPY>"The response is generic whether or not the account exists."</p>
            </div>
            <form class=FIELD_GROUP on:submit=move |event| {
                event.prevent_default();
                action.dispatch(ResendEmailVerification {
                    email: email.get_untracked(),
                    redirect_url: Some("/dashboard".to_string()),
                });
            }>
                <label class=FIELD>
                    <span>"Email"</span>
                    <input
                        class=INPUT
                        type="email"
                        autocomplete="email"
                        prop:value=move || email.get()
                        on:input=move |event| set_email.set(event_target_value(&event))
                    />
                </label>
                <button type="submit" class=BTN_AUTH_SUBMIT disabled=move || pending.get()>
                    "Send verification link"
                </button>
                <Show when=move || value.get().is_some()>
                    <p class=RESULT_LINE>{move || action_result_text(value.get())}</p>
                </Show>
                <Show when=move || value.get().is_some() && matches!(capture_enabled.get(), Some(Ok(true)))>
                    <p class=BANNER_SUCCESS>"Capture mode stores this message locally; it will not arrive in an external inbox."</p>
                    <button
                        type="button"
                        class=AUTH_SECONDARY
                        disabled=move || capture_pending.get()
                        on:click=move |_| {
                            capture_action.dispatch(LatestDevelopmentMail {
                                recipient: email.get_untracked(),
                                message_kind: "email-verification".to_owned(),
                            });
                        }
                    >
                        {move || if capture_pending.get() { "Looking for message" } else { "Open captured verification link" }}
                    </button>
                    <Show when=move || selected_action_error(capture_value.get()).is_some()>
                        <p class=AUTH_INLINE_ERROR>{move || selected_action_error(capture_value.get()).unwrap_or_default()}</p>
                    </Show>
                </Show>
            </form>
        </section>
    }
}

#[island]
pub fn ForgotPasswordForm() -> impl IntoView {
    let action = ServerAction::<StartPasswordReset>::new();
    let pending = action.pending();
    let value = action.value();
    let (email, set_email) = signal(String::new());
    let (client_error, set_client_error) = signal(None::<String>);

    let submit = move || {
        let email_value = email.get_untracked().trim().to_string();
        if let Err(error) = validate_email_only(&email_value) {
            set_client_error.set(Some(error));
            return;
        }
        set_client_error.set(None);
        action.dispatch(StartPasswordReset {
            email: email_value,
            redirect_url: Some("/dashboard".to_string()),
        });
    };

    view! {
        <section class=AUTH_FORM>
            <div>
                <p class=SECTION_LABEL>"Password reset"</p>
                <h1 class=AUTH_TITLE>"Recover access"</h1>
                <p class=AUTH_COPY>
                    "Enter your email and we will send reset instructions if an account exists."
                </p>
            </div>
            <form class=FIELD_GROUP on:submit=move |event| {
                event.prevent_default();
                submit();
            }>
                <label class=FIELD>
                    <span>"Email"</span>
                    <input
                        class=INPUT
                        type="email"
                        name="email"
                        autocomplete="username"
                        placeholder="name@company.com"
                        prop:value=move || email.get()
                        aria-invalid=move || client_error.get().is_some()
                        on:input=move |event| {
                            set_email.set(event_target_value(&event));
                            set_client_error.set(None);
                        }
                    />
                    <small>"For privacy, the response is the same even if no account exists."</small>
                </label>
                <p
                    class=BANNER_ERROR
                    hidden=move || client_error.get().is_none()
                >
                    {move || client_error.get().unwrap_or_default()}
                </p>
                <div hidden=move || value.get().is_none()>
                    <PasswordResetStartResult result=move || value.get() />
                </div>
                <button
                    type="submit"
                    class=BTN_AUTH_SUBMIT
                    disabled=move || pending.get()
                    aria-busy=move || if pending.get() { "true" } else { "false" }
                >
                    <span
                        class=AUTH_BUTTON_SPINNER
                        aria-hidden="true"
                        hidden=move || !pending.get()
                    ></span>
                    <span>{move || if pending.get() { "Sending reset link" } else { "Send reset link" }}</span>
                </button>
                <a class=AUTH_TEXT_LINK href="/login">"Return to sign in"</a>
            </form>
        </section>
    }
}

#[component]
pub fn PasswordResetStartResult(
    result: impl Fn() -> Option<Result<PasswordResetStartResponse, ServerFnError>>
    + Copy
    + Send
    + 'static,
) -> impl IntoView {
    view! {
        {move || match result() {
            Some(Ok(response)) => {
                let _ = response;
                view! {
                    <div class=BANNER_SUCCESS>
                        <p>"If an account exists, reset instructions are ready to send."</p>
                    </div>
                }.into_any()
            }
            Some(Err(error)) => view! { <p class=BANNER_ERROR>{server_error_text(error)}</p> }.into_any(),
            None => view! {}.into_any(),
        }}
    }
}

#[island]
pub fn ResetPasswordForm() -> impl IntoView {
    let action = ServerAction::<CompletePasswordReset>::new();
    let pending = action.pending();
    let value = action.value();
    let (password, set_password) = signal(String::new());
    let (client_error, set_client_error) = signal(None::<String>);

    Effect::new(move |_| {
        if let Some(Ok(response)) = value.get() {
            redirect_browser(&response.redirect_url);
        }
    });

    let submit = move || {
        let token = one_time_token_from_url();
        let password_value = password.get_untracked();
        if token.is_none() {
            set_client_error.set(Some("Reset token is missing.".to_string()));
            return;
        }
        if !(15..=128).contains(&password_value.chars().count()) {
            set_client_error.set(Some(
                "Password must contain 15 to 128 characters.".to_string(),
            ));
            return;
        }
        set_client_error.set(None);
        action.dispatch(CompletePasswordReset {
            token: token.unwrap_or_default(),
            password: password_value,
            redirect_url: Some("/dashboard".to_string()),
        });
    };

    view! {
        <section class=AUTH_FORM>
            <div>
                <p class=SECTION_LABEL>"Password reset"</p>
                <h1 class=AUTH_TITLE>"Choose a new password"</h1>
                <p class=AUTH_COPY>
                    "Use the reset link once. After the password changes, a new session is issued."
                </p>
            </div>
            <form class=FIELD_GROUP on:submit=move |event| {
                event.prevent_default();
                submit();
            }>
                <label class=FIELD>
                    <span>"New password"</span>
                    <input
                        class=INPUT
                        type="password"
                        name="password"
                        autocomplete="new-password"
                        placeholder="Enter your new password"
                        prop:value=move || password.get()
                        aria-invalid=move || client_error.get().is_some()
                        on:input=move |event| {
                            set_password.set(event_target_value(&event));
                            set_client_error.set(None);
                        }
                    />
                    <small>"Use 15 to 128 characters. Existing sessions should be reviewed after reset."</small>
                </label>
                <p
                    class=BANNER_ERROR
                    hidden=move || client_error.get().is_none()
                >
                    {move || client_error.get().unwrap_or_default()}
                </p>
                <p
                    class=BANNER_ERROR
                    hidden=move || selected_action_error(value.get()).is_none()
                >
                    {move || selected_action_error(value.get()).unwrap_or_default()}
                </p>
                <button
                    type="submit"
                    class=BTN_AUTH_SUBMIT
                    disabled=move || pending.get()
                    aria-busy=move || if pending.get() { "true" } else { "false" }
                >
                    <span
                        class=AUTH_BUTTON_SPINNER
                        aria-hidden="true"
                        hidden=move || !pending.get()
                    ></span>
                    <span>{move || if pending.get() { "Updating password" } else { "Reset password" }}</span>
                </button>
                <a class=AUTH_TEXT_LINK href="/login">"Return to sign in"</a>
            </form>
        </section>
    }
}

#[island]
pub fn InvitationAcceptForm() -> impl IntoView {
    let action = ServerAction::<AcceptOrganizationInvitation>::new();
    let pending = action.pending();
    let value = action.value();
    let (client_error, set_client_error) = signal(None::<String>);
    let (accepted_org, set_accepted_org) = signal(None::<OrganizationSummary>);
    // Keep the first hydrated render identical to the server render. Reading
    // window.location directly from the view made the token-present branch
    // differ between SSR (no window) and the browser during hydration.
    let (invitation_token, set_invitation_token) = signal(None::<String>);

    Effect::new(move |_| {
        set_invitation_token.set(one_time_token_from_url());
    });

    Effect::new(move |_| {
        if let Some(Ok(organization)) = value.get() {
            set_accepted_org.set(Some(organization));
        }
    });

    let submit = move || {
        let Some(token) = invitation_token.get_untracked() else {
            set_client_error.set(Some(
                "Invitation token is missing. Open the one-time link from your email.".to_string(),
            ));
            return;
        };
        set_client_error.set(None);
        action.dispatch(AcceptOrganizationInvitation { token });
    };

    view! {
        <section class=AUTH_FORM>
            <div>
                <p class=SECTION_LABEL>"Organization invite"</p>
                <h1 class=AUTH_TITLE>"Accept invitation"</h1>
                <p class=AUTH_COPY>
                    "Join the organization with the account you are signed in as. The invite email must match this account."
                </p>
            </div>
            <Show
                when=move || accepted_org.get().is_some()
                fallback=move || view! {
                    <div class=FIELD_GROUP>
                        <p
                            class=BANNER_ERROR
                            hidden=move || client_error.get().is_none()
                        >
                            {move || client_error.get().unwrap_or_default()}
                        </p>
                        <p
                            class=BANNER_ERROR
                            hidden=move || selected_action_error(value.get()).is_none()
                        >
                            {move || selected_action_error(value.get()).unwrap_or_default()}
                        </p>
                        <Show when=move || invitation_token.get().is_none()>
                            <p class=BANNER_ERROR>
                                "Open this page from the invitation email so the one-time token is present."
                            </p>
                        </Show>
                        <button
                            type="button"
                            class=BTN_AUTH_SUBMIT
                            disabled=move || pending.get() || invitation_token.get().is_none()
                            aria-busy=move || if pending.get() { "true" } else { "false" }
                            on:click=move |_| submit()
                        >
                            <span
                                class=AUTH_BUTTON_SPINNER
                                aria-hidden="true"
                                hidden=move || !pending.get()
                            ></span>
                            <span>
                                {move || {
                                    if pending.get() {
                                        "Accepting invitation"
                                    } else {
                                        "Accept invitation"
                                    }
                                }}
                            </span>
                        </button>
                        <a class=AUTH_TEXT_LINK href="/organizations">"Back to organizations"</a>
                    </div>
                }
            >
                <div class=BANNER_SUCCESS>
                    <p>
                        {move || {
                            accepted_org
                                .get()
                                .map(|org| {
                                    format!(
                                        "You joined {}. Role: {}.",
                                        org.name, org.current_user_role
                                    )
                                })
                                .unwrap_or_default()
                        }}
                    </p>
                    <div class=BUTTON_ROW>
                        <a class=BTN_PRIMARY href="/organizations">
                            "Open organizations"
                        </a>
                        <a class=BTN_SECONDARY href="/dashboard">"Dashboard"</a>
                    </div>
                </div>
            </Show>
        </section>
    }
}

/// Flat OAuth buttons for the login card (Apple-style alternative methods).
#[component]
pub fn OAuthProviderButtons() -> impl IntoView {
    let providers = browser_load(list_auth_providers);

    view! {
        <div class=AUTH_ALT_STACK>
            <For
                each=move || match providers.get() {
                    Some(Ok(providers)) => providers,
                    _ => Vec::new(),
                }
                key=|provider| provider.provider_id.clone()
                children=move |provider| view! {
                    <ProviderLoginButton
                        provider_id=provider.provider_id
                        label=provider.display_name
                    />
                }
            />
        </div>
    }
}

#[component]
pub fn OAuthProviderList() -> impl IntoView {
    view! {
        <section class=PANEL_COMPACT>
            <h2>"Sign in with a provider"</h2>
            <OAuthProviderButtons />
        </section>
    }
}

#[component]
pub fn ProviderLoginButton(provider_id: String, label: String) -> impl IntoView {
    let action = ServerAction::<StartOauthLogin>::new();
    let pending = action.pending();
    let value = action.value();
    let provider_for_submit = provider_id.clone();
    let label_for_view = label.clone();

    Effect::new(move |_| {
        if let Some(Ok(response)) = value.get() {
            redirect_browser(&response.authorization_url);
        }
    });

    let submit = move |_| {
        action.dispatch(StartOauthLogin {
            provider_id: provider_for_submit.clone(),
            redirect_url: Some(next_url()),
        });
    };

    view! {
        <button
            type="button"
            class=AUTH_ALT_BUTTON
            disabled=move || pending.get()
            on:click=submit
        >
            {move || if pending.get() {
                format!("Connecting to {label_for_view}…")
            } else {
                format!("Sign in with {label_for_view}")
            }}
        </button>
        <Show when=move || matches!(value.get(), Some(Err(_)))>
            <p class=AUTH_INLINE_ERROR>{move || action_result_text(value.get())}</p>
        </Show>
    }
}

/// Login-page optional block (unchanged gate).
#[island(lazy)]
pub fn OptionalPasskeyRegistration() -> impl IntoView {
    view! { <PasskeyManager /> }
}

#[island]
pub fn LogoutForm() -> impl IntoView {
    let action = ServerAction::<LogoutCurrentSession>::new();
    let pending = action.pending();
    let value = action.value();

    Effect::new(move |_| {
        if value.get().is_some_and(|result| result.is_ok()) {
            redirect_browser("/");
        }
    });

    view! {
        <div class=AUTH_ALT_STACK>
            <button
                type="button"
                class=BTN_SECONDARY
                disabled=move || pending.get()
                on:click=move |_| {
                    action.dispatch(LogoutCurrentSession {});
                }
            >
                "Log out"
            </button>
            <Show when=move || value.get().is_some()>
                <p class=RESULT_LINE>{move || action_result_text(value.get())}</p>
            </Show>
        </div>
    }
}

#[island]
pub fn LogoutButton() -> impl IntoView {
    let action = ServerAction::<LogoutCurrentSession>::new();
    let pending = action.pending();
    let value = action.value();

    Effect::new(move |_| {
        if value.get().is_some_and(|result| result.is_ok()) {
            redirect_browser("/");
        }
    });

    view! {
        <button
            type="button"
            class=BTN_SECONDARY
            disabled=move || pending.get()
            on:click=move |_| {
                action.dispatch(LogoutCurrentSession {});
            }
        >
            "Sign out"
        </button>
    }
}

#[island]
pub fn SessionSummary() -> impl IntoView {
    let session = browser_load(get_current_session);

    view! {
        <section class=PANEL>
            <div class="flex items-center justify-between gap-2">
                <div>
                    <p class=SECTION_LABEL>"Identity"</p>
                    <h2>"Current session"</h2>
                </div>
                <span
                    class="text-xs text-secondary"
                    hidden=move || !matches!(session.get(), Some(Ok(view)) if view.authenticated)
                >
                    {move || {
                        session
                            .get()
                            .and_then(Result::ok)
                            .map(|view| view.assurance.to_uppercase())
                            .unwrap_or_default()
                    }}
                </span>
            </div>
            <dl
                class="grid gap-1 text-[13px]"
                hidden=move || !matches!(session.get(), Some(Ok(view)) if view.authenticated)
            >
                <dt>"Tenant"</dt>
                <dd class="font-mono text-[12px] text-secondary">{move || session.get().and_then(Result::ok).and_then(|view| view.tenant_id).unwrap_or_else(|| "—".to_string())}</dd>
                <dt>"User"</dt>
                <dd class="font-mono text-[12px] text-secondary">{move || session.get().and_then(Result::ok).and_then(|view| view.user_id).unwrap_or_else(|| "—".to_string())}</dd>
                <dt>"Email"</dt>
                <dd>{move || session.get().and_then(Result::ok).and_then(|view| view.primary_email).unwrap_or_else(|| "—".to_string())}</dd>
            </dl>
            <p
                class=RESULT_LINE
                hidden=move || matches!(session.get(), Some(Ok(view)) if view.authenticated)
            >
                {move || match session.get() {
                    None => "Loading session details".to_string(),
                    Some(Ok(_)) => "No active session".to_string(),
                    Some(Err(error)) => server_error_text(error),
                }}
            </p>
        </section>
    }
}

#[island]
pub fn OAuthCallbackStatus() -> impl IntoView {
    let action = ServerAction::<CompleteOauthCallback>::new();
    let pending = action.pending();
    let value = action.value();

    view! {
        <section class=PANEL>
            <button
                type="button"
                class=BTN_SECONDARY
                disabled=move || pending.get()
                on:click=move |_| {
                    action.dispatch(CompleteOauthCallback {
                        provider_id: "unknown".to_string(),
                        code: None,
                        state: None,
                        redirect_url: Some(next_url()),
                    });
                }
            >
                "Complete callback"
            </button>
            <Show when=move || value.get().is_some()>
                <p class=RESULT_LINE>{move || action_result_text(value.get())}</p>
            </Show>
        </section>
    }
}
