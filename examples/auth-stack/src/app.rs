#![allow(unused_imports)]

use crate::contracts::{
    AuthCapabilities, AuthProviderSummary, AuthzCheckRequest, AuthzCheckResponse,
    AuthzModelWriteRequest, AuthzModelWriteResponse, EmailPasswordLoginRequest,
    EmailPasswordRegisterRequest, LoginCompletionResponse, LogoutResponse, OAuthCallbackRequest,
    OAuthStartResponse, PasskeyStartRequest, PasskeyStartResponse, PasskeyVerifyRequest,
    PasswordResetCompleteRequest, PasswordResetStartRequest, PasswordResetStartResponse,
    RelationshipTupleWriteRequest, RelationshipTupleWriteResponse, SessionView,
    SigningKeyListResponse, SigningKeyRotateRequest, SigningKeyRotateResponse,
};
use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{components::*, path};
use std::collections::BTreeMap;

#[cfg(feature = "hydrate")]
use leptos::task::spawn_local;
#[cfg(feature = "hydrate")]
use wasm_bindgen::prelude::*;
#[cfg(feature = "hydrate")]
use web_sys::window;

#[cfg(feature = "hydrate")]
#[wasm_bindgen(inline_js = r#"
function b64urlToBuffer(value) {
  const normalized = value.replace(/-/g, "+").replace(/_/g, "/");
  const padded = normalized + "===".slice((normalized.length + 3) % 4);
  const binary = atob(padded);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes.buffer;
}

function bufferToB64url(buffer) {
  const bytes = new Uint8Array(buffer);
  let binary = "";
  for (let index = 0; index < bytes.length; index += 1) {
    binary += String.fromCharCode(bytes[index]);
  }
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

function decodeCredentialDescriptors(descriptors) {
  if (!Array.isArray(descriptors)) {
    return descriptors;
  }
  return descriptors.map((descriptor) => ({
    ...descriptor,
    id: b64urlToBuffer(descriptor.id),
  }));
}

export function passkeySupported() {
  return Boolean(window.PublicKeyCredential && navigator.credentials);
}

export async function createPasskeyCredential(optionsJson) {
  const publicKey = JSON.parse(optionsJson);
  publicKey.challenge = b64urlToBuffer(publicKey.challenge);
  publicKey.user.id = b64urlToBuffer(publicKey.user.id);
  publicKey.excludeCredentials = decodeCredentialDescriptors(publicKey.excludeCredentials);

  const credential = await navigator.credentials.create({ publicKey });
  if (!credential) {
    throw new Error("No passkey credential was created.");
  }

  const transports = credential.response.getTransports
    ? credential.response.getTransports()
    : [];
  return JSON.stringify({
    id: bufferToB64url(credential.rawId),
    transports,
    attestationObject: bufferToB64url(credential.response.attestationObject),
    clientDataJSON: bufferToB64url(credential.response.clientDataJSON),
  });
}

export async function getPasskeyCredential(optionsJson) {
  const publicKey = JSON.parse(optionsJson);
  publicKey.challenge = b64urlToBuffer(publicKey.challenge);
  publicKey.allowCredentials = decodeCredentialDescriptors(publicKey.allowCredentials);

  const credential = await navigator.credentials.get({ publicKey });
  if (!credential) {
    throw new Error("No passkey credential was selected.");
  }

  const response = {
    id: bufferToB64url(credential.rawId),
    authenticatorData: bufferToB64url(credential.response.authenticatorData),
    signature: bufferToB64url(credential.response.signature),
    clientDataJSON: bufferToB64url(credential.response.clientDataJSON),
  };
  if (credential.response.userHandle) {
    response.userHandle = bufferToB64url(credential.response.userHandle);
  }
  return JSON.stringify(response);
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = passkeySupported)]
    fn passkey_supported() -> bool;

    #[wasm_bindgen(catch, js_name = createPasskeyCredential)]
    async fn create_passkey_credential(options_json: String) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, js_name = getPasskeyCredential)]
    async fn get_passkey_credential(options_json: String) -> Result<JsValue, JsValue>;
}

#[cfg(feature = "ssr")]
pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <AutoReload options=options.clone() />
                <HydrationScripts options=options.clone() root="" />
                <MetaTags />
            </head>
            <body>
                <App />
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    let fallback = || view! { <NotFoundPage /> }.into_view();

    view! {
        <Stylesheet id="leptos" href="/pkg/auth_stack.css" />
        <Meta name="description" content="Spin authentication and authorization stack for ddd_cqrs_es" />
        <Title text="Auth Stack" />

        <Router>
            <main class="auth-shell">
                <Routes fallback>
                    <Route path=path!("") view=LoginPage />
                    <Route path=path!("/login") view=LoginPage />
                    <Route path=path!("/register") view=RegisterPage />
                    <Route path=path!("/forgot-password") view=ForgotPasswordPage />
                    <Route path=path!("/reset-password") view=ResetPasswordPage />
                    <Route path=path!("/dashboard") view=DashboardPage />
                    <Route path=path!("/logout") view=LogoutPage />
                    <Route path=path!("/auth/callback/:provider") view=OAuthCallbackPage />
                    <Route path=path!("/auth/callback/:provider/error") view=OAuthCallbackErrorPage />
                    <Route path=path!("/auth/required") view=AuthRequiredPage />
                    <Route path=path!("/auth/forbidden") view=ForbiddenPage />
                    <Route path=path!("/auth/session-expired") view=SessionExpiredPage />
                    <Route path=path!("/auth/passkey-unsupported") view=PasskeyUnsupportedPage />
                    <Route path=path!("/account/security") view=AccountSecurityPage />
                    <Route path=path!("/admin/auth/signing-keys") view=SigningKeyAdminPage />
                    <Route path=path!("/admin/auth/providers") view=AuthProviderAdminPage />
                    <Route path=path!("/admin/auth/redirects") view=RedirectAllowlistPage />
                    <Route path=path!("/admin/authz/models") view=AuthzModelAdminPage />
                    <Route path=path!("/admin/authz/tuples") view=RelationshipTupleAdminPage />
                    <Route path=path!("/admin/authz/check") view=AuthzCheckPage />
                    <Route path=path!("/*any") view=NotFoundPage />
                </Routes>
            </main>
        </Router>
    }
}

#[component]
fn LoginPage() -> impl IntoView {
    view! {
        <div class="auth-page">
            <ExistingSessionRedirect />
            <section class="auth-card">
                <AuthBrand />
                <EmailPasswordAuthForm register_default=false />
                <OptionalLoginMethods />
            </section>
        </div>
    }
}

#[component]
fn RegisterPage() -> impl IntoView {
    view! {
        <div class="auth-page">
            <ExistingSessionRedirect />
            <section class="auth-card">
                <AuthBrand />
                <EmailPasswordAuthForm register_default=true />
                <OptionalLoginMethods />
            </section>
        </div>
    }
}

#[component]
fn ForgotPasswordPage() -> impl IntoView {
    view! {
        <div class="auth-page">
            <ExistingSessionRedirect />
            <section class="auth-card">
                <AuthBrand />
                <ForgotPasswordForm />
            </section>
        </div>
    }
}

#[component]
fn ResetPasswordPage() -> impl IntoView {
    view! {
        <div class="auth-page">
            <ExistingSessionRedirect />
            <section class="auth-card">
                <AuthBrand />
                <ResetPasswordForm />
            </section>
        </div>
    }
}

#[component]
fn AuthBrand() -> impl IntoView {
    view! {
        <div class="auth-brand">
            <span class="auth-logo" aria-hidden="true">"d"</span>
            <div>
                <p class="auth-brand-name">"ddd-auth"</p>
                <p class="auth-brand-meta">"Secure workspace access"</p>
            </div>
        </div>
    }
}

#[component]
fn ExistingSessionRedirect() -> impl IntoView {
    let session = Resource::new(|| (), |_| get_current_session());

    view! {
        <Suspense fallback=move || view! {}>
            {move || {
                if let Some(Ok(session)) = session.get()
                    && session.authenticated
                {
                    redirect_browser(&next_url());
                }
                view! {}
            }}
        </Suspense>
    }
}

#[component]
fn DashboardPage() -> impl IntoView {
    page_shell(
        "Dashboard",
        "Your authenticated workspace session is active.",
        view! {
            <SessionSummary />
            <section class="panel">
                <h2>"Workspace access"</h2>
                <p class="result-line">
                    "Use this page as the protected landing route for authenticated users."
                </p>
                <div class="actions">
                    <a class="link-button" href="/account/security">"Account security"</a>
                    <a class="link-button" href="/admin/auth/signing-keys">"Signing keys"</a>
                    <a class="link-button" href="/admin/authz/check">"Authorization check"</a>
                </div>
            </section>
        },
    )
}

#[component]
fn LogoutPage() -> impl IntoView {
    page_shell(
        "Log out",
        "End the current browser session.",
        view! { <LogoutForm /> },
    )
}

#[component]
fn OAuthCallbackPage() -> impl IntoView {
    page_shell(
        "Completing sign-in",
        "The provider callback will be verified by the server.",
        view! { <OAuthCallbackStatus /> },
    )
}

#[component]
fn OAuthCallbackErrorPage() -> impl IntoView {
    set_page_status(http::StatusCode::BAD_REQUEST);
    error_page_shell(
        "Sign-in failed",
        "The provider response could not be accepted.",
        view! { <ReturnToLoginLink /> },
    )
}

#[component]
fn AuthRequiredPage() -> impl IntoView {
    set_page_status(http::StatusCode::UNAUTHORIZED);
    error_page_shell(
        "Authentication required",
        "Sign in before continuing.",
        view! { <LoginRedirectLink /> },
    )
}

#[component]
fn ForbiddenPage() -> impl IntoView {
    set_page_status(http::StatusCode::FORBIDDEN);
    error_page_shell(
        "Access denied",
        "The current account cannot open this page.",
        view! {
            <div class="actions">
                <a class="link-button" href="/account/security">"Account security"</a>
                <LogoutForm />
            </div>
        },
    )
}

#[component]
fn SessionExpiredPage() -> impl IntoView {
    set_page_status(http::StatusCode::UNAUTHORIZED);
    error_page_shell(
        "Session expired",
        "Sign in again to continue.",
        view! { <LoginRedirectLink /> },
    )
}

#[component]
fn PasskeyUnsupportedPage() -> impl IntoView {
    error_page_shell(
        "Passkey unavailable",
        "Use email and password or an enabled provider.",
        view! { <OAuthProviderList /> },
    )
}

#[component]
fn AccountSecurityPage() -> impl IntoView {
    page_shell(
        "Account security",
        "Manage the current session.",
        view! {
            <SessionSummary />
            <OptionalPasskeyRegistration />
            <LogoutForm />
        },
    )
}

#[component]
fn AuthProviderAdminPage() -> impl IntoView {
    page_shell(
        "Auth providers",
        "Configure OAuth and OIDC providers.",
        view! { <ProviderConfigForm /> },
    )
}

#[component]
fn SigningKeyAdminPage() -> impl IntoView {
    page_shell(
        "Signing keys",
        "Rotate the active access-token signing key.",
        view! { <SigningKeyRotationForm /> },
    )
}

#[component]
fn RedirectAllowlistPage() -> impl IntoView {
    page_shell(
        "Redirect allowlist",
        "Restrict browser redirect targets.",
        view! { <RedirectAllowlistForm /> },
    )
}

#[component]
fn AuthzModelAdminPage() -> impl IntoView {
    page_shell(
        "Authorization models",
        "Publish and activate tenant models.",
        view! {
            <AuthorizationModelForm />
            <ActivateModelForm />
        },
    )
}

#[component]
fn RelationshipTupleAdminPage() -> impl IntoView {
    page_shell(
        "Relationship tuples",
        "Write and delete authorization tuples.",
        view! { <RelationshipTupleForm /> },
    )
}

#[component]
fn AuthzCheckPage() -> impl IntoView {
    page_shell(
        "Authorization check",
        "Evaluate a subject, relation, and object.",
        view! { <ManualAuthzCheckForm /> },
    )
}

#[component]
fn NotFoundPage() -> impl IntoView {
    set_page_status(http::StatusCode::NOT_FOUND);
    error_page_shell(
        "Not found",
        "This page does not exist.",
        view! { <ReturnToLoginLink /> },
    )
}

#[component]
fn EmailPasswordAuthForm(register_default: bool) -> impl IntoView {
    let login_action = ServerAction::<LoginEmailPassword>::new();
    let register_action = ServerAction::<RegisterEmailPassword>::new();
    let login_pending = login_action.pending();
    let register_pending = register_action.pending();
    let login_value = login_action.value();
    let register_value = register_action.value();
    let (register_mode, set_register_mode) = signal(register_default);
    let (email, set_email) = signal(String::new());
    let (password, set_password) = signal(String::new());
    let (client_error, set_client_error) = signal(None::<String>);

    Effect::new(move |_| {
        if let Some(Ok(response)) = login_value.get() {
            redirect_browser(&response.redirect_url);
        }
    });
    Effect::new(move |_| {
        if let Some(Ok(response)) = register_value.get() {
            redirect_browser(&response.redirect_url);
        }
    });

    let submit_credentials = move || {
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

    view! {
        <section class="auth-form">
            <div>
                <p class="auth-kicker">"Authentication"</p>
                <h1 class="auth-title">
                    {move || if register_mode.get() { "Create your workspace" } else { "Welcome back" }}
                </h1>
                <p class="auth-copy">
                    {move || if register_mode.get() {
                        "Set up a password-backed workspace session."
                    } else {
                        "Use your email and password to continue."
                    }}
                </p>
            </div>

            <div class="auth-mode-switch" role="tablist" aria-label="Authentication mode">
                <button
                    type="button"
                    class=move || if register_mode.get() {
                        "auth-mode-button"
                    } else {
                        "auth-mode-button auth-mode-button-active"
                    }
                    on:click=move |_| {
                        set_register_mode.set(false);
                        set_client_error.set(None);
                    }
                >
                    "Sign in"
                </button>
                <button
                    type="button"
                    class=move || if register_mode.get() {
                        "auth-mode-button auth-mode-button-active"
                    } else {
                        "auth-mode-button"
                    }
                    on:click=move |_| {
                        set_register_mode.set(true);
                        set_client_error.set(None);
                    }
                >
                    "Create workspace"
                </button>
            </div>

            <form class="auth-fields" on:submit=move |event| {
                event.prevent_default();
                submit_credentials();
            }>
                <label class="auth-field">
                    <span>"Email"</span>
                    <input
                        class="auth-input"
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
                </label>
                <label class="auth-field">
                    <span>"Password"</span>
                    <input
                        class="auth-input"
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
                    <small>{move || if register_mode.get() {
                        "Use at least 8 characters. Only a derived password hash is stored."
                    } else {
                        "Your session is issued by the local Spin auth service."
                    }}</small>
                </label>

                <Show when=move || client_error.get().is_some()>
                    <p class="auth-error">{move || client_error.get().unwrap_or_default()}</p>
                </Show>
                <Show when=move || selected_auth_error(
                    register_mode.get(),
                    login_value.get(),
                    register_value.get(),
                ).is_some()>
                    <p class="auth-error">
                        {move || selected_auth_error(
                            register_mode.get(),
                            login_value.get(),
                            register_value.get(),
                        ).unwrap_or_default()}
                    </p>
                </Show>

                <button
                    type="submit"
                    class="auth-submit"
                    disabled=move || login_pending.get() || register_pending.get()
                    aria-busy=move || if login_pending.get() || register_pending.get() { "true" } else { "false" }
                >
                    <Show when=move || login_pending.get() || register_pending.get()>
                        <span class="auth-button-spinner" aria-hidden="true"></span>
                    </Show>
                    <span>
                        {move || if login_pending.get() || register_pending.get() {
                            if register_mode.get() { "Creating workspace" } else { "Signing in" }
                        } else if register_mode.get() {
                            "Create workspace"
                        } else {
                            "Continue"
                        }}
                    </span>
                </button>
                <a class="auth-text-link" href="/forgot-password">"Forgot password?"</a>
            </form>
            <p class="auth-trust-copy">
                "Protected by server-side validation, httpOnly session cookies, and tenant-scoped authorization checks."
            </p>
        </section>
    }
}

#[component]
fn ForgotPasswordForm() -> impl IntoView {
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
        <section class="auth-form">
            <div>
                <p class="auth-kicker">"Password reset"</p>
                <h1 class="auth-title">"Recover access"</h1>
                <p class="auth-copy">
                    "Enter your email and the local auth service will issue a short-lived reset link."
                </p>
            </div>
            <form class="auth-fields" on:submit=move |event| {
                event.prevent_default();
                submit();
            }>
                <label class="auth-field">
                    <span>"Email"</span>
                    <input
                        class="auth-input"
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
                <Show when=move || client_error.get().is_some()>
                    <p class="auth-error">{move || client_error.get().unwrap_or_default()}</p>
                </Show>
                <Show when=move || value.get().is_some()>
                    <PasswordResetStartResult result=move || value.get() />
                </Show>
                <button
                    type="submit"
                    class="auth-submit"
                    disabled=move || pending.get()
                    aria-busy=move || if pending.get() { "true" } else { "false" }
                >
                    <Show when=move || pending.get()>
                        <span class="auth-button-spinner" aria-hidden="true"></span>
                    </Show>
                    <span>{move || if pending.get() { "Sending reset link" } else { "Send reset link" }}</span>
                </button>
                <a class="auth-text-link" href="/login">"Return to sign in"</a>
            </form>
        </section>
    }
}

#[component]
fn PasswordResetStartResult(
    result: impl Fn() -> Option<Result<PasswordResetStartResponse, ServerFnError>>
    + Copy
    + Send
    + 'static,
) -> impl IntoView {
    view! {
        {move || match result() {
            Some(Ok(response)) => {
                let reset_url = response.reset_url;
                if let Some(reset_url) = reset_url {
                    view! {
                        <div class="auth-success">
                            <p>"If the account exists, a reset link is ready."</p>
                            <a class="auth-text-link" href=reset_url>"Open reset link"</a>
                        </div>
                    }.into_any()
                } else {
                    view! {
                        <div class="auth-success">
                            <p>"If the account exists, a reset link is ready."</p>
                        </div>
                    }.into_any()
                }
            }
            Some(Err(error)) => view! { <p class="auth-error">{server_error_text(error)}</p> }.into_any(),
            None => view! {}.into_any(),
        }}
    }
}

#[component]
fn ResetPasswordForm() -> impl IntoView {
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
        let token = reset_token_from_url();
        let password_value = password.get_untracked();
        if token.is_none() {
            set_client_error.set(Some("Reset token is missing.".to_string()));
            return;
        }
        if password_value.len() < 8 {
            set_client_error.set(Some("Password must be at least 8 characters.".to_string()));
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
        <section class="auth-form">
            <div>
                <p class="auth-kicker">"Password reset"</p>
                <h1 class="auth-title">"Choose a new password"</h1>
                <p class="auth-copy">
                    "Use the reset link once. After the password changes, a new session is issued."
                </p>
            </div>
            <form class="auth-fields" on:submit=move |event| {
                event.prevent_default();
                submit();
            }>
                <label class="auth-field">
                    <span>"New password"</span>
                    <input
                        class="auth-input"
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
                    <small>"Use at least 8 characters. Existing sessions should be reviewed after reset."</small>
                </label>
                <Show when=move || client_error.get().is_some()>
                    <p class="auth-error">{move || client_error.get().unwrap_or_default()}</p>
                </Show>
                <Show when=move || selected_action_error(value.get()).is_some()>
                    <p class="auth-error">
                        {move || selected_action_error(value.get()).unwrap_or_default()}
                    </p>
                </Show>
                <button
                    type="submit"
                    class="auth-submit"
                    disabled=move || pending.get()
                    aria-busy=move || if pending.get() { "true" } else { "false" }
                >
                    <Show when=move || pending.get()>
                        <span class="auth-button-spinner" aria-hidden="true"></span>
                    </Show>
                    <span>{move || if pending.get() { "Updating password" } else { "Reset password" }}</span>
                </button>
                <a class="auth-text-link" href="/login">"Return to sign in"</a>
            </form>
        </section>
    }
}

#[component]
fn OptionalLoginMethods() -> impl IntoView {
    let capabilities = Resource::new(|| (), |_| get_auth_capabilities());

    view! {
        <Suspense fallback=move || view! {}>
            {move || match capabilities.get() {
                Some(Ok(capabilities))
                    if (capabilities.oauth_enabled && !capabilities.providers.is_empty())
                        || capabilities.passkeys_enabled =>
                {
                    let show_oauth = capabilities.oauth_enabled && !capabilities.providers.is_empty();
                    let show_passkeys = capabilities.passkeys_enabled;
                    view! {
                        <section class="optional-methods">
                            <Show when=move || show_oauth>
                                <OAuthProviderList />
                            </Show>
                            <Show when=move || show_passkeys>
                                <PasskeyLoginForm />
                            </Show>
                        </section>
                    }
                    .into_any()
                }
                _ => view! {}.into_any(),
            }}
        </Suspense>
    }
}

#[component]
fn OAuthProviderList() -> impl IntoView {
    let providers = Resource::new(|| (), |_| list_auth_providers());

    view! {
        <section class="panel compact-panel">
            <h2>"Connected providers"</h2>
            <Suspense fallback=move || view! { <p class="result-line">"Loading providers"</p> }>
                {move || match providers.get() {
                    Some(Ok(providers)) if providers.is_empty() => view! {
                        <p class="result-line">"No providers are enabled."</p>
                    }.into_any(),
                    Some(Ok(providers)) => view! {
                        <div class="button-grid">
                            <For
                                each=move || providers.clone()
                                key=|provider| provider.provider_id.clone()
                                children=move |provider| view! {
                                    <ProviderLoginButton
                                        provider_id=provider.provider_id
                                        label=provider.display_name
                                    />
                                }
                            />
                        </div>
                    }.into_any(),
                    Some(Err(error)) => view! {
                        <p class="result-line">{server_error_text(error)}</p>
                    }.into_any(),
                    None => view! { <p class="result-line">"Loading providers"</p> }.into_any(),
                }}
            </Suspense>
        </section>
    }
}

#[component]
fn ProviderLoginButton(provider_id: String, label: String) -> impl IntoView {
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
        <div class="action-stack">
            <button type="button" class="primary-button" disabled=move || pending.get() on:click=submit>
                {label_for_view.clone()}
            </button>
            <Show when=move || value.get().is_some()>
                <p class="result-line">{move || action_result_text(value.get())}</p>
            </Show>
        </div>
    }
}

#[component]
fn PasskeyLoginForm() -> impl IntoView {
    let start_action = ServerAction::<StartPasskeyLogin>::new();
    let verify_action = ServerAction::<VerifyPasskeyLogin>::new();
    let start_pending = start_action.pending();
    let verify_pending = verify_action.pending();
    let start_value = start_action.value();
    let verify_value = verify_action.value();
    let (email, set_email) = signal(String::new());
    let (client_error, set_client_error) = signal(None::<String>);

    Effect::new(move |_| {
        if let Some(Ok(response)) = start_value.get() {
            #[cfg(feature = "hydrate")]
            {
                if !passkey_supported() {
                    redirect_browser("/auth/passkey-unsupported");
                    return;
                }

                let verify_action = verify_action.clone();
                let set_client_error = set_client_error;
                let challenge_id = response.challenge_id;
                let options_json = response.public_key_options_json;
                let redirect_url = Some(next_url());
                set_client_error.set(None);
                spawn_local(async move {
                    match get_passkey_credential(options_json).await {
                        Ok(value) => match passkey_js_string(value) {
                            Ok(credential_json) => {
                                verify_action.dispatch(VerifyPasskeyLogin {
                                    challenge_id,
                                    credential_json,
                                    redirect_url,
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

    Effect::new(move |_| {
        if let Some(Ok(response)) = verify_value.get() {
            redirect_browser(&response.redirect_url);
        }
    });

    let submit = move |_| {
        set_client_error.set(None);
        start_action.dispatch(StartPasskeyLogin {
            email: optional_text(email.get_untracked()),
            redirect_url: Some(next_url()),
        });
    };

    view! {
        <section class="panel">
            <h2>"Passkey"</h2>
            <label>
                <span>"Email"</span>
                <input
                    type="email"
                    autocomplete="username webauthn"
                    prop:value=move || email.get()
                    on:input=move |event| set_email.set(event_target_value(&event))
                />
            </label>
            <div class="actions">
                <button type="button" class="secondary-button" disabled=move || start_pending.get() || verify_pending.get() on:click=submit>
                    {move || if start_pending.get() || verify_pending.get() { "Waiting for passkey" } else { "Use passkey" }}
                </button>
            </div>
            <Show when=move || start_value.get().is_some()>
                <p class="result-line">{move || action_result_text(start_value.get())}</p>
            </Show>
            <Show when=move || verify_value.get().is_some()>
                <p class="result-line">{move || action_result_text(verify_value.get())}</p>
            </Show>
            <Show when=move || client_error.get().is_some()>
                <p class="error-banner">{move || client_error.get().unwrap_or_default()}</p>
            </Show>
        </section>
    }
}

#[component]
fn OptionalPasskeyRegistration() -> impl IntoView {
    let capabilities = Resource::new(|| (), |_| get_auth_capabilities());

    view! {
        <Suspense fallback=move || view! {}>
            {move || match capabilities.get() {
                Some(Ok(capabilities)) if capabilities.passkeys_enabled => {
                    view! { <PasskeyRegistrationForm /> }.into_any()
                }
                _ => view! {}.into_any(),
            }}
        </Suspense>
    }
}

#[component]
fn PasskeyRegistrationForm() -> impl IntoView {
    let start_action = ServerAction::<StartPasskeyRegistration>::new();
    let verify_action = ServerAction::<VerifyPasskeyRegistration>::new();
    let start_pending = start_action.pending();
    let verify_pending = verify_action.pending();
    let start_value = start_action.value();
    let verify_value = verify_action.value();
    let (email, set_email) = signal(String::new());
    let (client_error, set_client_error) = signal(None::<String>);

    Effect::new(move |_| {
        if let Some(Ok(response)) = start_value.get() {
            #[cfg(feature = "hydrate")]
            {
                if !passkey_supported() {
                    redirect_browser("/auth/passkey-unsupported");
                    return;
                }

                let verify_action = verify_action.clone();
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
                                    redirect_url: Some("/account/security".to_string()),
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

    Effect::new(move |_| {
        if let Some(Ok(response)) = verify_value.get() {
            redirect_browser(&response.redirect_url);
        }
    });

    let submit = move |_| {
        set_client_error.set(None);
        start_action.dispatch(StartPasskeyRegistration {
            email: optional_text(email.get_untracked()),
            redirect_url: Some("/account/security".to_string()),
        });
    };

    view! {
        <section class="panel">
            <h2>"Register passkey"</h2>
            <label>
                <span>"Email"</span>
                <input
                    type="email"
                    autocomplete="username"
                    prop:value=move || email.get()
                    on:input=move |event| set_email.set(event_target_value(&event))
                />
            </label>
            <div class="actions">
                <button type="button" class="secondary-button" disabled=move || start_pending.get() || verify_pending.get() on:click=submit>
                    {move || if start_pending.get() || verify_pending.get() { "Waiting for passkey" } else { "Register passkey" }}
                </button>
            </div>
            <Show when=move || start_value.get().is_some()>
                <p class="result-line">{move || action_result_text(start_value.get())}</p>
            </Show>
            <Show when=move || verify_value.get().is_some()>
                <p class="result-line">{move || action_result_text(verify_value.get())}</p>
            </Show>
            <Show when=move || client_error.get().is_some()>
                <p class="error-banner">{move || client_error.get().unwrap_or_default()}</p>
            </Show>
        </section>
    }
}

#[component]
fn LogoutForm() -> impl IntoView {
    let action = ServerAction::<LogoutCurrentSession>::new();
    let pending = action.pending();
    let value = action.value();

    view! {
        <div class="action-stack">
            <button
                type="button"
                class="secondary-button"
                disabled=move || pending.get()
                on:click=move |_| {
                    action.dispatch(LogoutCurrentSession {});
                }
            >
                "Log out"
            </button>
            <Show when=move || value.get().is_some()>
                <p class="result-line">{move || action_result_text(value.get())}</p>
            </Show>
        </div>
    }
}

#[component]
fn SessionSummary() -> impl IntoView {
    let session = Resource::new(|| (), |_| get_current_session());

    view! {
        <section class="panel">
            <h2>"Current session"</h2>
            <Suspense fallback=move || view! { <p class="result-line">"Loading"</p> }>
                {move || match session.get() {
                    Some(Ok(view)) if view.authenticated => view! {
                        <dl class="kv">
                            <dt>"Tenant"</dt><dd>{view.tenant_id.unwrap_or_default()}</dd>
                            <dt>"User"</dt><dd>{view.user_id.unwrap_or_default()}</dd>
                            <dt>"Email"</dt><dd>{view.primary_email.unwrap_or_default()}</dd>
                        </dl>
                    }.into_any(),
                    Some(Ok(_)) => view! { <p class="result-line">"No active session"</p> }.into_any(),
                    Some(Err(error)) => view! { <p class="result-line">{error.to_string()}</p> }.into_any(),
                    None => view! { <p class="result-line">"Loading"</p> }.into_any(),
                }}
            </Suspense>
        </section>
    }
}

#[component]
fn ProviderConfigForm() -> impl IntoView {
    let action = ServerAction::<SaveAuthProvider>::new();
    let pending = action.pending();
    let value = action.value();
    let (provider_id, set_provider_id) = signal("google".to_string());
    let (enabled, set_enabled) = signal(true);

    let submit = move |_| {
        action.dispatch(SaveAuthProvider {
            provider_id: provider_id.get_untracked(),
            enabled: enabled.get_untracked(),
        });
    };

    view! {
        <section class="panel">
            <h2>"Provider"</h2>
            <label>
                <span>"Provider id"</span>
                <input
                    type="text"
                    prop:value=move || provider_id.get()
                    on:input=move |event| set_provider_id.set(event_target_value(&event))
                />
            </label>
            <label class="inline-field">
                <input
                    type="checkbox"
                    prop:checked=move || enabled.get()
                    on:change=move |event| set_enabled.set(event_target_checked(&event))
                />
                <span>"Enabled"</span>
            </label>
            <button type="button" class="secondary-button" disabled=move || pending.get() on:click=submit>
                "Save provider"
            </button>
            <Show when=move || value.get().is_some()>
                <p class="result-line">{move || action_result_text(value.get())}</p>
            </Show>
        </section>
    }
}

#[component]
fn SigningKeyRotationForm() -> impl IntoView {
    let rotate_action = ServerAction::<RotateSigningKey>::new();
    let pending = rotate_action.pending();
    let value = rotate_action.value();
    let (admin_token, set_admin_token) = signal(String::new());
    let (kid, set_kid) = signal("auth-stack-next-hs256".to_string());
    let (retire_previous, set_retire_previous) = signal(true);
    let keys = Resource::new(
        move || admin_token.get(),
        |token| async move {
            if token.trim().is_empty() {
                Ok(SigningKeyListResponse { keys: Vec::new() })
            } else {
                list_signing_keys(token).await
            }
        },
    );

    let submit = move |_| {
        rotate_action.dispatch(RotateSigningKey {
            admin_token: admin_token.get_untracked(),
            kid: kid.get_untracked(),
            retire_previous: retire_previous.get_untracked(),
        });
    };

    view! {
        <section class="panel">
            <h2>"Signing key rotation"</h2>
            <label>
                <span>"Admin token"</span>
                <input
                    type="password"
                    autocomplete="off"
                    prop:value=move || admin_token.get()
                    on:input=move |event| set_admin_token.set(event_target_value(&event))
                />
            </label>
            <Suspense fallback=move || view! { <p class="result-line">"Loading keys"</p> }>
                {move || match keys.get() {
                    Some(Ok(response)) if response.keys.is_empty() => view! {
                        <p class="result-line">"Enter the configured admin token to inspect signing keys."</p>
                    }.into_any(),
                    Some(Ok(response)) => view! {
                        <dl class="kv">
                            <For
                                each=move || response.keys.clone()
                                key=|key| key.kid.clone()
                                children=move |key| view! {
                                    <dt>{key.kid}</dt>
                                    <dd>{format!("{} / {}{}", key.alg, key.status, if key.active { " / active" } else { "" })}</dd>
                                }
                            />
                        </dl>
                    }.into_any(),
                    Some(Err(error)) => view! {
                        <p class="result-line">{server_error_text(error)}</p>
                    }.into_any(),
                    None => view! { <p class="result-line">"Loading keys"</p> }.into_any(),
                }}
            </Suspense>
            <label>
                <span>"Target key id"</span>
                <input
                    type="text"
                    prop:value=move || kid.get()
                    on:input=move |event| set_kid.set(event_target_value(&event))
                />
            </label>
            <label class="inline-field">
                <input
                    type="checkbox"
                    prop:checked=move || retire_previous.get()
                    on:change=move |event| set_retire_previous.set(event_target_checked(&event))
                />
                <span>"Retire previous active key"</span>
            </label>
            <button type="button" class="secondary-button" disabled=move || pending.get() on:click=submit>
                "Rotate key"
            </button>
            <Show when=move || value.get().is_some()>
                <p class="result-line">{move || action_result_text(value.get())}</p>
            </Show>
        </section>
    }
}

#[component]
fn RedirectAllowlistForm() -> impl IntoView {
    let action = ServerAction::<SaveRedirectAllowlist>::new();
    let pending = action.pending();
    let value = action.value();
    let (redirects_json, set_redirects_json) = signal("[\"/account/security\"]".to_string());

    let submit = move |_| {
        action.dispatch(SaveRedirectAllowlist {
            redirects_json: redirects_json.get_untracked(),
        });
    };

    view! {
        <section class="panel">
            <h2>"Allowed redirects"</h2>
            <textarea
                rows="5"
                prop:value=move || redirects_json.get()
                on:input=move |event| set_redirects_json.set(event_target_value(&event))
            />
            <button type="button" class="secondary-button" disabled=move || pending.get() on:click=submit>
                "Save allowlist"
            </button>
            <Show when=move || value.get().is_some()>
                <p class="result-line">{move || action_result_text(value.get())}</p>
            </Show>
        </section>
    }
}

#[component]
fn AuthorizationModelForm() -> impl IntoView {
    let action = ServerAction::<WriteAuthorizationModel>::new();
    let pending = action.pending();
    let value = action.value();
    let (model_id, set_model_id) = signal("model_1".to_string());
    let (schema_json, set_schema_json) = signal("{\"schema_version\":\"1.0\"}".to_string());

    let submit = move |_| {
        action.dispatch(WriteAuthorizationModel {
            model_id: model_id.get_untracked(),
            schema_json: schema_json.get_untracked(),
        });
    };

    view! {
        <section class="panel">
            <h2>"Model"</h2>
            <label>
                <span>"Model id"</span>
                <input
                    type="text"
                    prop:value=move || model_id.get()
                    on:input=move |event| set_model_id.set(event_target_value(&event))
                />
            </label>
            <textarea
                rows="7"
                prop:value=move || schema_json.get()
                on:input=move |event| set_schema_json.set(event_target_value(&event))
            />
            <button type="button" class="secondary-button" disabled=move || pending.get() on:click=submit>
                "Write model"
            </button>
            <Show when=move || value.get().is_some()>
                <p class="result-line">{move || action_result_text(value.get())}</p>
            </Show>
        </section>
    }
}

#[component]
fn ActivateModelForm() -> impl IntoView {
    let action = ServerAction::<ActivateAuthorizationModel>::new();
    let pending = action.pending();
    let value = action.value();
    let (model_id, set_model_id) = signal("model_1".to_string());

    let submit = move |_| {
        action.dispatch(ActivateAuthorizationModel {
            model_id: model_id.get_untracked(),
        });
    };

    view! {
        <section class="panel">
            <h2>"Active model"</h2>
            <label>
                <span>"Model id"</span>
                <input
                    type="text"
                    prop:value=move || model_id.get()
                    on:input=move |event| set_model_id.set(event_target_value(&event))
                />
            </label>
            <button type="button" class="secondary-button" disabled=move || pending.get() on:click=submit>
                "Activate"
            </button>
            <Show when=move || value.get().is_some()>
                <p class="result-line">{move || action_result_text(value.get())}</p>
            </Show>
        </section>
    }
}

#[component]
fn RelationshipTupleForm() -> impl IntoView {
    let write_action = ServerAction::<WriteRelationshipTuples>::new();
    let delete_action = ServerAction::<DeleteRelationshipTuples>::new();
    let write_pending = write_action.pending();
    let delete_pending = delete_action.pending();
    let write_value = write_action.value();
    let delete_value = delete_action.value();
    let (tuples_json, set_tuples_json) = signal("[]".to_string());

    let write = move |_| {
        write_action.dispatch(WriteRelationshipTuples {
            tuples_json: tuples_json.get_untracked(),
        });
    };
    let delete = move |_| {
        delete_action.dispatch(DeleteRelationshipTuples {
            tuples_json: tuples_json.get_untracked(),
        });
    };

    view! {
        <section class="panel">
            <h2>"Tuples"</h2>
            <textarea
                rows="7"
                prop:value=move || tuples_json.get()
                on:input=move |event| set_tuples_json.set(event_target_value(&event))
            />
            <div class="actions">
                <button type="button" class="secondary-button" disabled=move || write_pending.get() on:click=write>
                    "Write"
                </button>
                <button type="button" class="secondary-button" disabled=move || delete_pending.get() on:click=delete>
                    "Delete"
                </button>
            </div>
            <Show when=move || write_value.get().is_some()>
                <p class="result-line">{move || action_result_text(write_value.get())}</p>
            </Show>
            <Show when=move || delete_value.get().is_some()>
                <p class="result-line">{move || action_result_text(delete_value.get())}</p>
            </Show>
        </section>
    }
}

#[component]
fn ManualAuthzCheckForm() -> impl IntoView {
    let action = ServerAction::<RunAuthorizationCheck>::new();
    let pending = action.pending();
    let value = action.value();
    let (tenant, set_tenant) = signal("tenant:default".to_string());
    let (subject, set_subject) = signal("user:alice".to_string());
    let (object, set_object) = signal("project:demo".to_string());
    let (relation, set_relation) = signal("viewer".to_string());

    let submit = move |_| {
        action.dispatch(RunAuthorizationCheck {
            tenant: tenant.get_untracked(),
            subject: subject.get_untracked(),
            object: object.get_untracked(),
            relation: relation.get_untracked(),
        });
    };

    view! {
        <section class="panel">
            <h2>"Check"</h2>
            <label>
                <span>"Tenant"</span>
                <input type="text" prop:value=move || tenant.get() on:input=move |event| set_tenant.set(event_target_value(&event)) />
            </label>
            <label>
                <span>"Subject"</span>
                <input type="text" prop:value=move || subject.get() on:input=move |event| set_subject.set(event_target_value(&event)) />
            </label>
            <label>
                <span>"Object"</span>
                <input type="text" prop:value=move || object.get() on:input=move |event| set_object.set(event_target_value(&event)) />
            </label>
            <label>
                <span>"Relation"</span>
                <input type="text" prop:value=move || relation.get() on:input=move |event| set_relation.set(event_target_value(&event)) />
            </label>
            <button type="button" class="secondary-button" disabled=move || pending.get() on:click=submit>
                "Run check"
            </button>
            <Show when=move || value.get().is_some()>
                <p class="result-line">{move || action_result_text(value.get())}</p>
            </Show>
        </section>
    }
}

#[component]
fn OAuthCallbackStatus() -> impl IntoView {
    let action = ServerAction::<CompleteOauthCallback>::new();
    let pending = action.pending();
    let value = action.value();

    view! {
        <section class="panel">
            <button
                type="button"
                class="secondary-button"
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
                <p class="result-line">{move || action_result_text(value.get())}</p>
            </Show>
        </section>
    }
}

#[component]
fn LoginRedirectLink() -> impl IntoView {
    view! { <a class="link-button" href=move || format!("/login?next={}", next_url())>"Sign in"</a> }
}

#[component]
fn ReturnToLoginLink() -> impl IntoView {
    view! { <a class="link-button" href="/login">"Return to sign in"</a> }
}

fn page_shell(
    title: &'static str,
    subtitle: &'static str,
    children: impl IntoView + 'static,
) -> impl IntoView {
    view! {
        <div class="page">
            <nav class="top-nav">
                <a href="/login">"Login"</a>
                <a href="/dashboard">"Dashboard"</a>
                <a href="/account/security">"Security"</a>
                <a href="/admin/authz/check">"Authz check"</a>
            </nav>
            <section class="page-header">
                <p>"ddd-auth / ddd-authz"</p>
                <h1>{title}</h1>
                <span>{subtitle}</span>
            </section>
            <section class="page-grid">
                {children}
            </section>
        </div>
    }
}

fn error_page_shell(
    title: &'static str,
    subtitle: &'static str,
    children: impl IntoView + 'static,
) -> impl IntoView {
    view! {
        <div class="grid min-h-[100dvh] place-items-center bg-[#f6f3ee] px-5 py-12 text-[#1b2228]">
            <section class="w-full max-w-[480px] rounded-[18px] border border-[#ded8cf] bg-[#fffdf9] p-8 shadow-[0_28px_80px_-54px_rgba(31,27,22,0.55)] sm:p-10">
                <p class="text-xs font-bold uppercase tracking-[0.14em] text-[#80786d]">"ddd-auth / ddd-authz"</p>
                <h1 class="mt-4 text-[2rem] font-semibold leading-[1.05] tracking-[-0.035em] text-[#151b20] sm:text-[2.35rem]">{title}</h1>
                <p class="mt-3 text-[0.95rem] leading-6 text-[#68635b]">{subtitle}</p>
                <div class="mt-7 flex flex-wrap gap-3">{children}</div>
            </section>
        </div>
    }
}

fn selected_auth_error(
    register_mode: bool,
    login_result: Option<Result<LoginCompletionResponse, ServerFnError>>,
    register_result: Option<Result<LoginCompletionResponse, ServerFnError>>,
) -> Option<String> {
    let selected = if register_mode {
        register_result
    } else {
        login_result
    };
    match selected {
        Some(Err(error)) => Some(server_error_text(error)),
        _ => None,
    }
}

fn selected_action_error<T>(result: Option<Result<T, ServerFnError>>) -> Option<String> {
    match result {
        Some(Err(error)) => Some(server_error_text(error)),
        _ => None,
    }
}

fn validate_email_only(email: &str) -> Result<(), String> {
    if email.trim().is_empty() {
        return Err("Email is required.".to_string());
    }
    if !email.contains('@') || !email.contains('.') {
        return Err("Enter a valid email address.".to_string());
    }
    Ok(())
}

fn validate_login_form(email: &str, password: &str, register_mode: bool) -> Result<(), String> {
    validate_email_only(email)?;
    if password.is_empty() {
        return Err("Password is required.".to_string());
    }
    if register_mode && password.len() < 8 {
        return Err("Password must be at least 8 characters.".to_string());
    }
    Ok(())
}

fn server_error_text(error: ServerFnError) -> String {
    let text = error.to_string();
    text.strip_prefix("error running server function: ")
        .unwrap_or(&text)
        .to_string()
}

fn action_result_text<T>(result: Option<Result<T, ServerFnError>>) -> String {
    match result {
        Some(Ok(_)) => "Request accepted".to_string(),
        Some(Err(error)) => server_error_text(error),
        None => String::new(),
    }
}

fn optional_text(value: String) -> Option<String> {
    let value = value.trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}

#[cfg(feature = "hydrate")]
fn passkey_js_string(value: JsValue) -> Result<String, String> {
    value
        .as_string()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "Passkey response was not readable.".to_string())
}

#[cfg(feature = "hydrate")]
fn passkey_js_error(error: JsValue) -> String {
    error
        .as_string()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Passkey prompt was cancelled or unavailable.".to_string())
}

fn next_url() -> String {
    #[cfg(feature = "hydrate")]
    {
        if let Some(window) = window()
            && let Ok(search) = window.location().search()
            && let Some(value) = search.strip_prefix("?next=")
            && value.starts_with('/')
            && !value.starts_with("//")
            && value != "/login"
        {
            return value.to_string();
        }
    }
    "/dashboard".to_string()
}

fn reset_token_from_url() -> Option<String> {
    #[cfg(feature = "hydrate")]
    {
        if let Some(window) = window()
            && let Ok(search) = window.location().search()
        {
            return search
                .trim_start_matches('?')
                .split('&')
                .find_map(|part| part.strip_prefix("token="))
                .map(ToOwned::to_owned)
                .filter(|value| !value.trim().is_empty());
        }
    }
    None
}

fn redirect_browser(url: &str) {
    #[cfg(feature = "hydrate")]
    {
        if let Some(window) = window() {
            let location = window.location();
            if location.replace(url).is_err() {
                let _ = location.set_href(url);
            }
        }
    }
    let _ = url;
}

fn set_page_status(status: http::StatusCode) {
    #[cfg(feature = "ssr")]
    {
        if let Some(resp) = use_context::<leptos_wasi::response::ResponseOptions>() {
            resp.set_status(status);
        }
    }
    let _ = status;
}

#[cfg(feature = "ssr")]
fn server_fn_error(error: crate::error::AuthStackError) -> ServerFnError {
    if error.is_client_error() {
        tracing::warn!(
            error = %error,
            error_code = error.public_code(),
            "auth server function rejected request"
        );
    } else {
        tracing::error!(
            error = %error,
            error_code = error.public_code(),
            "auth server function failed"
        );
    }
    error.server_fn_error()
}

#[cfg(feature = "ssr")]
fn current_session_id_from_cookie() -> Option<String> {
    use http::header::COOKIE;

    let parts = use_context::<http::request::Parts>()?;
    let cookie_header = parts.headers.get(COOKIE)?.to_str().ok()?;
    session_id_from_cookie_header(cookie_header)
}

#[cfg(feature = "ssr")]
fn session_id_from_cookie_header(cookie_header: &str) -> Option<String> {
    cookie_header.split(';').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        if name == "ddd_auth_session" && !value.trim().is_empty() {
            Some(value.trim().to_string())
        } else {
            None
        }
    })
}

#[cfg(feature = "ssr")]
async fn set_session_cookie(response: &LoginCompletionResponse) {
    use http::HeaderValue;
    use http::header::SET_COOKIE;

    let Some(session_id) = response.session_id.as_deref() else {
        return;
    };
    let cookie_value = crate::application::session_cookie_header_value(
        session_id,
        Some(3600),
        crate::application::session_cookie_secure_enabled().await,
    );
    let Ok(cookie) = HeaderValue::from_str(&cookie_value) else {
        return;
    };
    if let Some(resp) = use_context::<leptos_wasi::response::ResponseOptions>() {
        resp.append_header(SET_COOKIE, cookie);
    }
}

#[cfg(any(feature = "ssr", test))]
fn browser_login_response(mut response: LoginCompletionResponse) -> LoginCompletionResponse {
    response.session_id = None;
    response.access_token = None;
    response.refresh_token = None;
    response
}

#[cfg(feature = "ssr")]
async fn clear_session_cookie() {
    use http::HeaderValue;
    use http::header::SET_COOKIE;

    let cookie_value = crate::application::expired_session_cookie_header_value(
        crate::application::session_cookie_secure_enabled().await,
    );
    let Ok(cookie) = HeaderValue::from_str(&cookie_value) else {
        return;
    };
    if let Some(resp) = use_context::<leptos_wasi::response::ResponseOptions>() {
        resp.append_header(SET_COOKIE, cookie);
    }
}

#[server(prefix = "/api/ui")]
pub async fn get_auth_capabilities() -> Result<AuthCapabilities, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::auth_capabilities()
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn register_email_password(
    email: String,
    password: String,
    redirect_url: Option<String>,
) -> Result<LoginCompletionResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response = crate::application::register_email_password(EmailPasswordRegisterRequest {
            email,
            password,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)?;
        set_session_cookie(&response).await;
        Ok(browser_login_response(response))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (email, password, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn login_email_password(
    email: String,
    password: String,
    redirect_url: Option<String>,
) -> Result<LoginCompletionResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response = crate::application::login_email_password(EmailPasswordLoginRequest {
            email,
            password,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)?;
        set_session_cookie(&response).await;
        Ok(browser_login_response(response))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (email, password, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn start_password_reset(
    email: String,
    redirect_url: Option<String>,
) -> Result<PasswordResetStartResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::start_password_reset(PasswordResetStartRequest {
            email,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (email, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn complete_password_reset(
    token: String,
    password: String,
    redirect_url: Option<String>,
) -> Result<LoginCompletionResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response = crate::application::complete_password_reset(PasswordResetCompleteRequest {
            token,
            password,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)?;
        set_session_cookie(&response).await;
        Ok(browser_login_response(response))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (token, password, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_auth_providers() -> Result<Vec<AuthProviderSummary>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_auth_providers()
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn get_current_session() -> Result<SessionView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::get_current_session_for(current_session_id_from_cookie())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn require_authenticated_route() -> Result<SessionView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::require_authenticated_route_for(current_session_id_from_cookie())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn require_authorized_route(permission: String) -> Result<SessionView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::require_authorized_route_for(
            &permission,
            current_session_id_from_cookie(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = permission;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn start_passkey_registration(
    email: Option<String>,
    redirect_url: Option<String>,
) -> Result<PasskeyStartResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::start_passkey_registration(PasskeyStartRequest {
            email,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (email, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn verify_passkey_registration(
    challenge_id: String,
    credential_json: String,
    redirect_url: Option<String>,
) -> Result<LoginCompletionResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response = crate::application::verify_passkey_registration(PasskeyVerifyRequest {
            challenge_id,
            credential_json,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)?;
        set_session_cookie(&response).await;
        Ok(browser_login_response(response))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (challenge_id, credential_json, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn start_passkey_login(
    email: Option<String>,
    redirect_url: Option<String>,
) -> Result<PasskeyStartResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::start_passkey_login(PasskeyStartRequest {
            email,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (email, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn verify_passkey_login(
    challenge_id: String,
    credential_json: String,
    redirect_url: Option<String>,
) -> Result<LoginCompletionResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response = crate::application::verify_passkey_login(PasskeyVerifyRequest {
            challenge_id,
            credential_json,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)?;
        set_session_cookie(&response).await;
        Ok(browser_login_response(response))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (challenge_id, credential_json, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn start_oauth_login(
    provider_id: String,
    redirect_url: Option<String>,
) -> Result<OAuthStartResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::start_oauth_login(provider_id, redirect_url)
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (provider_id, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn complete_oauth_callback(
    provider_id: String,
    code: Option<String>,
    state: Option<String>,
    redirect_url: Option<String>,
) -> Result<LoginCompletionResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response = crate::application::complete_oauth_callback(OAuthCallbackRequest {
            provider_id,
            code,
            state,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)?;
        set_session_cookie(&response).await;
        Ok(browser_login_response(response))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (provider_id, code, state, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn logout_current_session() -> Result<LogoutResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response = crate::application::logout_session(current_session_id_from_cookie())
            .await
            .map_err(server_fn_error)?;
        clear_session_cookie().await;
        Ok(response)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn save_auth_provider(
    provider_id: String,
    enabled: bool,
) -> Result<AuthProviderSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::save_auth_provider_config(provider_id, enabled)
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (provider_id, enabled);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn save_redirect_allowlist(redirects_json: String) -> Result<bool, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::save_redirect_allowlist(redirects_json)
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = redirects_json;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_signing_keys(admin_token: String) -> Result<SigningKeyListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_signing_keys(optional_text(admin_token))
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = admin_token;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn rotate_signing_key(
    admin_token: String,
    kid: String,
    retire_previous: bool,
) -> Result<SigningKeyRotateResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::rotate_signing_key(SigningKeyRotateRequest {
            admin_token: optional_text(admin_token),
            kid,
            retire_previous: Some(retire_previous),
        })
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (admin_token, kid, retire_previous);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn write_authorization_model(
    model_id: String,
    schema_json: String,
) -> Result<AuthzModelWriteResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::write_authorization_model(AuthzModelWriteRequest {
            model_id,
            schema_json,
        })
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (model_id, schema_json);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn activate_authorization_model(
    model_id: String,
) -> Result<AuthzModelWriteResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::activate_authorization_model(model_id)
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = model_id;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn write_relationship_tuples(
    tuples_json: String,
) -> Result<RelationshipTupleWriteResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::write_relationship_tuples(RelationshipTupleWriteRequest { tuples_json })
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = tuples_json;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn delete_relationship_tuples(
    tuples_json: String,
) -> Result<RelationshipTupleWriteResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::delete_relationship_tuples(RelationshipTupleWriteRequest {
            tuples_json,
        })
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = tuples_json;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn run_authorization_check(
    tenant: String,
    subject: String,
    object: String,
    relation: String,
) -> Result<AuthzCheckResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::check_authorization(AuthzCheckRequest {
            tenant,
            subject,
            object,
            relation,
            context: BTreeMap::new(),
        })
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (tenant, subject, object, relation);
        unreachable!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_login_response_removes_browser_visible_tokens() {
        let response = LoginCompletionResponse {
            authenticated: true,
            redirect_url: "/dashboard".to_string(),
            session_id: Some("session_123".to_string()),
            access_token: Some("access-token".to_string()),
            refresh_token: Some("refresh-token".to_string()),
            expires_in_seconds: 3600,
        };

        let redacted = browser_login_response(response);

        assert!(redacted.authenticated);
        assert_eq!(redacted.redirect_url, "/dashboard");
        assert_eq!(redacted.expires_in_seconds, 3600);
        assert_eq!(redacted.session_id, None);
        assert_eq!(redacted.access_token, None);
        assert_eq!(redacted.refresh_token, None);
    }
}
