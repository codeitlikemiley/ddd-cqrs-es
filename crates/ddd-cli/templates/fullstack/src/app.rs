#![allow(unused_imports)]
#![allow(clippy::unused_unit)] // Leptos `view! {}` expands to intentional unit views.
#![allow(clippy::unit_arg)] // Empty Leptos views intentionally pass unit to `into_any`.

use crate::contracts::{
    AcceptedResponse, AccountSessionListResponse, AdminUserListResponse, AuditEventListResponse,
    AuthCapabilities, AuthProviderSummary, AuthorizationCapabilitiesResponse, CapturedMailResponse,
    EmailPasswordLoginRequest, EmailPasswordRegisterRequest, EmailVerificationCompleteRequest,
    EmailVerificationResendRequest, HealthStatusResponse, InvitationCreateRequest,
    InvitationListResponse, LoginCompletionResponse, LogoutResponse, MembershipListResponse,
    MfaCodeRequest, MfaEnrollConfirmResponse, MfaEnrollStartResponse, MfaStatusResponse,
    OAuthCallbackRequest, OAuthStartResponse, OrganizationCreateRequest, OrganizationListResponse,
    PasskeyStartRequest, PasskeyStartResponse, PasskeyVerifyRequest, PasswordChangeRequest,
    PasswordResetCompleteRequest, PasswordResetStartRequest, PasswordResetStartResponse,
    PolicyPublishRequest, PolicyVersionListResponse, RoleListResponse, RoleUpsertRequest,
    SessionRevokeRequest, SessionView, SigningKeyListResponse, SigningKeyRotateRequest,
    SigningKeyRotateResponse,
};
use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{components::*, path};

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

export function afterIslandHydration() {
  return new Promise((resolve) => setTimeout(resolve, 0));
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
    #[wasm_bindgen(catch, js_name = afterIslandHydration)]
    async fn after_island_hydration() -> Result<JsValue, JsValue>;

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
                <HydrationScripts options=options.clone() islands=true root="" />
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
        <Stylesheet id="leptos" href="/pkg/fullstack_app.css" />
        <Meta name="description" content="Spin authentication and authorization stack for ddd_cqrs_es" />
        <Title text="Auth Stack" />

        <Router>
            <main class="auth-shell">
                <Routes fallback>
                    <Route path=path!("") view=HomePage />
                    <Route path=path!("/login") view=LoginPage />
                    <Route path=path!("/register") view=RegisterPage />
                    <Route path=path!("/forgot-password") view=ForgotPasswordPage />
                    <Route path=path!("/reset-password") view=ResetPasswordPage />
                    <Route path=path!("/verify-email") view=VerifyEmailPage />
                    <Route path=path!("/verify-email/pending") view=VerificationPendingPage />
                    <Route path=path!("/verify-email/resend") view=ResendVerificationPage />
                    <Route path=path!("/dashboard") view=DashboardPage />
                    <Route path=path!("/logout") view=LogoutPage />
                    <Route path=path!("/auth/callback/:provider") view=OAuthCallbackPage />
                    <Route path=path!("/auth/callback/:provider/error") view=OAuthCallbackErrorPage />
                    <Route path=path!("/auth/required") view=AuthRequiredPage />
                    <Route path=path!("/auth/forbidden") view=ForbiddenPage />
                    <Route path=path!("/auth/session-expired") view=SessionExpiredPage />
                    <Route path=path!("/auth/passkey-unsupported") view=PasskeyUnsupportedPage />
                    <Route path=path!("/account/security") view=AccountSecurityPage />
                    <Route path=path!("/account/profile") view=AccountProfilePage />
                    <Route path=path!("/account/password") view=AccountPasswordPage />
                    <Route path=path!("/account/providers") view=AccountProvidersPage />
                    <Route path=path!("/account/passkeys") view=AccountPasskeysPage />
                    <Route path=path!("/account/mfa") view=AccountMfaPage />
                    <Route path=path!("/account/sessions") view=AccountSessionsPage />
                    <Route path=path!("/organizations") view=OrganizationsPage />
                    <Route path=path!("/organizations/settings") view=OrganizationSettingsPage />
                    <Route path=path!("/organizations/members") view=OrganizationMembersPage />
                    <Route path=path!("/organizations/invitations") view=OrganizationInvitationsPage />
                    <Route path=path!("/organizations/roles") view=OrganizationRolesPage />
                    <Route path=path!("/organizations/permissions") view=OrganizationPermissionsPage />
                    <Route path=path!("/organizations/audit") view=OrganizationAuditPage />
                    <Route path=path!("/admin/users") view=AdminUsersPage />
                    <Route path=path!("/admin/health") view=AdminHealthPage />
                    <Route path=path!("/admin/policies") view=AdminPoliciesPage />
                    <Route path=path!("/admin/auth/signing-keys") view=SigningKeyAdminPage />
                    <Route path=path!("/admin/auth/providers") view=AuthProviderAdminPage />
                    <Route path=path!("/admin/auth/redirects") view=RedirectAllowlistPage />
                    <Route path=path!("/admin/authorization/policy") view=AuthorizationPolicyPage />
                    <Route path=path!("/*any") view=NotFoundPage />
                </Routes>
            </main>
        </Router>
    }
}

#[component]
fn HomePage() -> impl IntoView {
    page_shell(
        "Production fullstack Rust",
        "Leptos islands, trusted authentication, embedded Cedar, DDD persistence, REST, and Spin gRPC in one component.",
        view! {
            <section class="panel">
                <h2>"Start securely"</h2>
                <p class="result-line">
                    "Register a global account, verify the email, then create or join an organization."
                </p>
                <div class="actions">
                    <a class="link-button" href="/register">"Create account"</a>
                    <a class="link-button" href="/login">"Sign in"</a>
                </div>
            </section>
            <section class="panel">
                <h2>"Transport parity"</h2>
                <p class="result-line">
                    "The same application services back browser server functions, REST endpoints, and authenticated gRPC services."
                </p>
            </section>
        },
    )
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
fn VerifyEmailPage() -> impl IntoView {
    view! {
        <div class="auth-page">
            <section class="auth-card">
                <AuthBrand />
                <EmailVerificationForm />
            </section>
        </div>
    }
}

#[component]
fn VerificationPendingPage() -> impl IntoView {
    view! {
        <div class="auth-page">
            <section class="auth-card">
                <AuthBrand />
                <section class="auth-form">
                    <div>
                        <p class="auth-kicker">"Email verification"</p>
                        <h1 class="auth-title">"Check your inbox"</h1>
                        <p class="auth-copy">
                            "Your account is pending. Open the one-time verification link before signing in."
                        </p>
                    </div>
                    <p class="auth-notice">
                        "Local capture mode keeps messages on this machine. Start the app with `make dev` to run delivery automatically."
                    </p>
                    <a class="auth-text-link" href="/verify-email/resend">"Send another verification link"</a>
                </section>
            </section>
        </div>
    }
}

#[component]
fn ResendVerificationPage() -> impl IntoView {
    view! {
        <div class="auth-page">
            <section class="auth-card">
                <AuthBrand />
                <ResendVerificationForm />
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
                <p class="auth-brand-name">"wasi-auth"</p>
                <p class="auth-brand-meta">"Secure workspace access"</p>
            </div>
        </div>
    }
}

#[island]
fn ExistingSessionRedirect() -> impl IntoView {
    let session = browser_load(get_current_session);

    view! {
        <div class="client-data-slot">
            {move || {
                if let Some(Ok(session)) = session.get()
                    && session.authenticated
                {
                    redirect_browser(&next_url());
                }
                view! {}
            }}
        </div>
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
                    <a class="link-button" href="/organizations">"Organizations"</a>
                    <a class="link-button" href="/account/security">"Account security"</a>
                    <a class="link-button" href="/organizations/audit">"Audit activity"</a>
                    <a class="link-button" href="/admin/health">"System health"</a>
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

#[island(lazy)]
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

#[island(lazy)]
fn AuthorizationPolicyPage() -> impl IntoView {
    let capabilities = browser_load(get_authorization_capabilities);
    page_shell(
        "Authorization policy",
        "Inspect the active embedded Cedar provider. Policy publication is restricted to MFA-authenticated system administrators.",
        view! {
            <section class="panel">
                <h2>"Active provider"</h2>
                <div class="client-data-slot">
                    {move || capabilities.get().map(|result| match result {
                        Ok(value) => view! {
                            <dl class="kv">
                                <dt>"Provider"</dt><dd>{value.provider}</dd>
                                <dt>"Maximum batch"</dt><dd>{value.max_batch_checks}</dd>
                                <dt>"Resource listing"</dt><dd>{value.list_resources}</dd>
                                <dt>"Consistency tokens"</dt><dd>{value.consistency_tokens}</dd>
                            </dl>
                        }.into_any(),
                        Err(error) => view! {
                            <p class="result-line">{server_error_text(error)}</p>
                        }.into_any(),
                    })}
                </div>
            </section>
        },
    )
}

#[component]
fn AccountProfilePage() -> impl IntoView {
    page_shell(
        "Profile",
        "Review the global account and selected organization context.",
        view! { <SessionSummary /> },
    )
}

#[component]
fn AccountPasswordPage() -> impl IntoView {
    page_shell(
        "Password",
        "Change a password through the authenticated credential workflow, or use the one-time reset flow.",
        view! { <ChangePasswordForm /> },
    )
}

#[island(lazy)]
fn AccountProvidersPage() -> impl IntoView {
    page_shell(
        "Linked providers",
        "Review enabled OAuth providers for this deployment.",
        view! { <OAuthProviderList /> },
    )
}

#[component]
fn AccountPasskeysPage() -> impl IntoView {
    page_shell(
        "Passkeys",
        "Register a phishing-resistant credential and establish MFA-level assurance.",
        view! { <OptionalPasskeyRegistration /> },
    )
}

#[component]
fn AccountMfaPage() -> impl IntoView {
    page_shell(
        "Multi-factor authentication",
        "Enroll TOTP, save one-time recovery codes, and step this session up to AAL2.",
        view! { <MfaManager /> },
    )
}

#[island(lazy)]
fn MfaManager() -> impl IntoView {
    let status = browser_load(get_mfa_status);
    let start = ServerAction::<StartTotpEnrollment>::new();
    let confirm = ServerAction::<ConfirmTotpEnrollment>::new();
    let verify = ServerAction::<VerifyTotpStepUp>::new();
    let recover = ServerAction::<VerifyRecoveryCode>::new();
    let (totp_code, set_totp_code) = signal(String::new());
    let (recovery_code, set_recovery_code) = signal(String::new());

    view! {
        <section class="panel">
            <h2>"MFA status"</h2>
            <dl class="kv" hidden=move || !matches!(status.get(), Some(Ok(_)))>
                <dt>"TOTP"</dt>
                <dd>{move || status.get().and_then(Result::ok).map(|value| if value.totp_enrolled { "Enrolled" } else { "Not enrolled" }).unwrap_or_default()}</dd>
                <dt>"Recovery codes"</dt>
                <dd>{move || status.get().and_then(Result::ok).map(|value| value.recovery_codes_remaining).unwrap_or_default()}</dd>
                <dt>"Current assurance"</dt>
                <dd>{move || status.get().and_then(Result::ok).map(|value| value.assurance).unwrap_or_default()}</dd>
            </dl>
            <p class="result-line" hidden=move || matches!(status.get(), Some(Ok(_)))>
                {move || match status.get() {
                    None => "Loading MFA status".to_string(),
                    Some(Ok(_)) => String::new(),
                    Some(Err(error)) => server_error_text(error),
                }}
            </p>
            <button
                type="button"
                class="primary-button"
                disabled=move || start.pending().get()
                on:click=move |_| { start.dispatch(StartTotpEnrollment {}); }
            >"Start TOTP enrollment"</button>
            <div hidden=move || start.value().get().is_none()>
                {move || match start.value().get() {
                    Some(Ok(value)) => view! {
                        <div class="compact-panel">
                            <h3>"Add this account to your authenticator"</h3>
                            <p class="result-line">"Secret (shown once): "<code>{value.secret_base32}</code></p>
                            <p class="result-line"><code>{value.provisioning_uri}</code></p>
                        </div>
                    }.into_any(),
                    Some(Err(error)) => view! { <p class="error-banner">{server_error_text(error)}</p> }.into_any(),
                    None => ().into_any(),
                }}
            </div>
        </section>
        <section class="panel">
            <h2>"Confirm or step up with TOTP"</h2>
            <label><span>"Authenticator code"</span><input
                inputmode="numeric"
                autocomplete="one-time-code"
                maxlength="8"
                prop:value=move || totp_code.get()
                on:input=move |event| set_totp_code.set(event_target_value(&event))
            /></label>
            <div class="button-row">
                <button type="button" class="primary-button" disabled=move || confirm.pending().get() on:click=move |_| {
                    confirm.dispatch(ConfirmTotpEnrollment { code: totp_code.get_untracked() });
                }>"Confirm enrollment"</button>
                <button type="button" class="secondary-button" disabled=move || verify.pending().get() on:click=move |_| {
                    verify.dispatch(VerifyTotpStepUp { code: totp_code.get_untracked() });
                }>"Step up session"</button>
            </div>
            <div hidden=move || confirm.value().get().is_none()>
                {move || match confirm.value().get() {
                    Some(Ok(value)) => view! {
                        <div class="compact-panel">
                            <h3>"Recovery codes — save now"</h3>
                            <p class="result-line">"Each code works once and cannot be shown again."</p>
                            <ul><For each=move || value.recovery_codes.clone() key=|code| code.clone() children=move |code| view! { <li><code>{code}</code></li> } /></ul>
                        </div>
                    }.into_any(),
                    Some(Err(error)) => view! { <p class="error-banner">{server_error_text(error)}</p> }.into_any(),
                    None => ().into_any(),
                }}
            </div>
            <p class="result-line" hidden=move || verify.value().get().is_none()>
                {move || action_result_text(verify.value().get())}
            </p>
        </section>
        <section class="panel">
            <h2>"Use a recovery code"</h2>
            <label><span>"Recovery code"</span><input
                autocomplete="one-time-code"
                maxlength="32"
                prop:value=move || recovery_code.get()
                on:input=move |event| set_recovery_code.set(event_target_value(&event))
            /></label>
            <button type="button" class="secondary-button" disabled=move || recover.pending().get() on:click=move |_| {
                recover.dispatch(VerifyRecoveryCode { code: recovery_code.get_untracked() });
            }>"Use recovery code"</button>
            <p class="result-line" hidden=move || recover.value().get().is_none()>
                {move || action_result_text(recover.value().get())}
            </p>
        </section>
    }
}

#[component]
fn AccountSessionsPage() -> impl IntoView {
    page_shell(
        "Active sessions",
        "Review and revoke browser access.",
        view! { <AccountSessionManager /> },
    )
}

#[island]
fn ChangePasswordForm() -> impl IntoView {
    let action = ServerAction::<ChangePassword>::new();
    let pending = action.pending();
    let value = action.value();
    let (current_password, set_current_password) = signal(String::new());
    let (new_password, set_new_password) = signal(String::new());
    view! {
        <section class="panel">
            <h2>"Change password"</h2>
            <p class="result-line">"A successful change revokes every other session and sends a security notification."</p>
            <label><span>"Current password"</span><input type="password" autocomplete="current-password" prop:value=move || current_password.get() on:input=move |event| set_current_password.set(event_target_value(&event)) /></label>
            <label><span>"New password"</span><input type="password" autocomplete="new-password" prop:value=move || new_password.get() on:input=move |event| set_new_password.set(event_target_value(&event)) /></label>
            <button type="button" class="primary-button" disabled=move || pending.get() on:click=move |_| {
                action.dispatch(ChangePassword {
                    current_password: current_password.get_untracked(),
                    new_password: new_password.get_untracked(),
                });
            }>"Change password"</button>
            <Show when=move || value.get().is_some()><p class="result-line">{move || action_result_text(value.get())}</p></Show>
            <a class="auth-text-link" href="/forgot-password">"Use one-time password reset instead"</a>
        </section>
    }
}

#[island(lazy)]
fn AccountSessionManager() -> impl IntoView {
    let sessions = browser_load(list_account_sessions);
    let revoke_action = ServerAction::<RevokeAccountSession>::new();
    view! {
        <section class="panel">
            <h2>"Active sessions"</h2>
            <div class="client-data-slot">
                {move || match sessions.get() {
                    Some(Ok(response)) => view! { <div class="action-stack"><For
                        each=move || response.sessions.clone()
                        key=|session| session.session_id.clone()
                        children=move |session| {
                            let session_id = session.session_id.clone();
                            view! { <article class="compact-panel">
                                <h3>{if session.current { "Current session" } else { "Session" }}</h3>
                                <p class="result-line">{format!("{} / expires {}", session.assurance, session.expires_at_ms)}</p>
                                <button type="button" class="secondary-button" on:click=move |_| {
                                    revoke_action.dispatch(RevokeAccountSession { session_id: session_id.clone() });
                                }>"Revoke"</button>
                            </article> }
                        }
                    /></div> }.into_any(),
                    Some(Err(error)) => view! { <p class="error-banner">{server_error_text(error)}</p> }.into_any(),
                    None => view! { <p class="result-line">"Loading sessions"</p> }.into_any(),
                }}
            </div>
            <Show when=move || revoke_action.value().get().is_some()><p class="result-line">{move || action_result_text(revoke_action.value().get())}</p></Show>
        </section>
    }
}

#[island(lazy)]
fn OrganizationsPage() -> impl IntoView {
    let organizations = browser_load(list_organizations);
    let create_action = ServerAction::<CreateOrganization>::new();
    let create_pending = create_action.pending();
    let create_value = create_action.value();
    let select_action = ServerAction::<SelectOrganization>::new();
    let (name, set_name) = signal(String::new());

    page_shell(
        "Organizations",
        "Create, select, and manage tenant-scoped workspaces.",
        view! {
            <section class="panel">
                <h2>"Your organizations"</h2>
                <div class="client-data-slot">
                    {move || match organizations.get() {
                        Some(Ok(response)) if response.organizations.is_empty() => view! {
                            <p class="result-line">"No organization yet. Create the first one below."</p>
                        }.into_any(),
                        Some(Ok(response)) => view! {
                            <div class="action-stack">
                                <For
                                    each=move || response.organizations.clone()
                                    key=|organization| organization.organization_id.clone()
                                    children=move |organization| {
                                        let organization_id = organization.organization_id.clone();
                                        view! {
                                            <article class="compact-panel">
                                                <h3>{organization.name}</h3>
                                                <p class="result-line">{format!("Role: {}", organization.current_user_role)}</p>
                                                <button
                                                    type="button"
                                                    class="secondary-button"
                                                    on:click=move |_| {
                                                        select_action.dispatch(SelectOrganization {
                                                            organization_id: organization_id.clone(),
                                                        });
                                                    }
                                                >"Select"</button>
                                            </article>
                                        }
                                    }
                                />
                            </div>
                        }.into_any(),
                        Some(Err(error)) => view! { <p class="error-banner">{server_error_text(error)}</p> }.into_any(),
                        None => view! { <p class="result-line">"Loading organizations"</p> }.into_any(),
                    }}
                </div>
            </section>
            <section class="panel">
                <h2>"Create organization"</h2>
                <label>
                    <span>"Name"</span>
                    <input
                        type="text"
                        maxlength="120"
                        prop:value=move || name.get()
                        on:input=move |event| set_name.set(event_target_value(&event))
                    />
                </label>
                <button
                    type="button"
                    class="primary-button"
                    disabled=move || create_pending.get()
                    on:click=move |_| {
                        create_action.dispatch(CreateOrganization {
                            name: name.get_untracked(),
                        });
                    }
                >"Create"</button>
                <Show when=move || create_value.get().is_some()>
                    <p class="result-line">{move || action_result_text(create_value.get())}</p>
                </Show>
            </section>
        },
    )
}

#[component]
fn OrganizationSettingsPage() -> impl IntoView {
    page_shell(
        "Organization settings",
        "The selected tenant comes from the verified session, never from an untrusted form alone.",
        view! { <SessionSummary /> <OrganizationLinks /> },
    )
}

#[island(lazy)]
fn OrganizationMembersPage() -> impl IntoView {
    let members = browser_load(list_current_organization_members);
    page_shell(
        "Members",
        "Review active, blocked, and removed organization memberships.",
        view! {
            <section class="panel">
                <h2>"Memberships"</h2>
                <div class="client-data-slot">
                    {move || match members.get() {
                        Some(Ok(response)) => view! {
                            <dl class="kv">
                                <For
                                    each=move || response.memberships.clone()
                                    key=|membership| membership.user_id.clone()
                                    children=move |membership| view! {
                                        <dt>{membership.primary_email}</dt>
                                        <dd>{format!("{} / {}", membership.role_id, membership.status)}</dd>
                                    }
                                />
                            </dl>
                        }.into_any(),
                        Some(Err(error)) => view! { <p class="error-banner">{server_error_text(error)}</p> }.into_any(),
                        None => view! { <p class="result-line">"Loading members"</p> }.into_any(),
                    }}
                </div>
            </section>
            <OrganizationLinks />
        },
    )
}

#[island(lazy)]
fn OrganizationInvitationsPage() -> impl IntoView {
    let invitations = browser_load(list_current_organization_invitations);
    let invite_action = ServerAction::<InviteCurrentOrganizationMember>::new();
    let invite_pending = invite_action.pending();
    let invite_value = invite_action.value();
    let (email, set_email) = signal(String::new());
    let (role_id, set_role_id) = signal("member".to_owned());
    page_shell(
        "Invitations",
        "One-time invitation values are mailed and only their hashes are persisted.",
        view! {
            <section class="panel">
                <h2>"Invite member"</h2>
                <label><span>"Email"</span><input type="email" prop:value=move || email.get() on:input=move |event| set_email.set(event_target_value(&event)) /></label>
                <label><span>"Role"</span><input type="text" prop:value=move || role_id.get() on:input=move |event| set_role_id.set(event_target_value(&event)) /></label>
                <button type="button" class="primary-button" disabled=move || invite_pending.get() on:click=move |_| {
                    invite_action.dispatch(InviteCurrentOrganizationMember {
                        email: email.get_untracked(),
                        role_id: role_id.get_untracked(),
                    });
                }>"Send invitation"</button>
                <Show when=move || invite_value.get().is_some()><p class="result-line">{move || action_result_text(invite_value.get())}</p></Show>
            </section>
            <section class="panel">
                <h2>"Invitation status"</h2>
                <div class="client-data-slot">
                    {move || match invitations.get() {
                        Some(Ok(response)) => view! {
                            <dl class="kv"><For each=move || response.invitations.clone() key=|invitation| invitation.invitation_id.clone() children=move |invitation| view! {
                                <dt>{invitation.email}</dt><dd>{format!("{} / {}", invitation.role_id, invitation.status)}</dd>
                            } /></dl>
                        }.into_any(),
                        Some(Err(error)) => view! { <p class="error-banner">{server_error_text(error)}</p> }.into_any(),
                        None => view! { <p class="result-line">"Loading invitations"</p> }.into_any(),
                    }}
                </div>
            </section>
        },
    )
}

#[island(lazy)]
fn OrganizationRolesPage() -> impl IntoView {
    let roles = browser_load(list_current_organization_roles);
    let upsert_action = ServerAction::<UpsertCurrentOrganizationRole>::new();
    let (role_id, set_role_id) = signal(String::new());
    let (name, set_name) = signal(String::new());
    let (permissions, set_permissions) = signal("organization.view,counter.view".to_owned());
    page_shell(
        "Roles",
        "Built-in roles are immutable; custom roles use the bounded tenant permission catalog.",
        view! {
            <section class="panel">
                <h2>"Role catalog"</h2>
                <div class="client-data-slot">
                    {move || match roles.get() {
                        Some(Ok(response)) => view! { <dl class="kv"><For each=move || response.roles.clone() key=|role| role.role_id.clone() children=move |role| view! {
                            <dt>{role.name}</dt><dd>{format!("{} permissions{}", role.permissions.len(), if role.built_in { " / built-in" } else { "" })}</dd>
                        } /></dl> }.into_any(),
                        Some(Err(error)) => view! { <p class="error-banner">{server_error_text(error)}</p> }.into_any(),
                        None => view! { <p class="result-line">"Loading roles"</p> }.into_any(),
                    }}
                </div>
            </section>
            <section class="panel">
                <h2>"Custom role"</h2>
                <label><span>"Role id"</span><input type="text" prop:value=move || role_id.get() on:input=move |event| set_role_id.set(event_target_value(&event)) /></label>
                <label><span>"Name"</span><input type="text" prop:value=move || name.get() on:input=move |event| set_name.set(event_target_value(&event)) /></label>
                <label><span>"Comma-separated permissions"</span><input type="text" prop:value=move || permissions.get() on:input=move |event| set_permissions.set(event_target_value(&event)) /></label>
                <button type="button" class="secondary-button" on:click=move |_| {
                    upsert_action.dispatch(UpsertCurrentOrganizationRole {
                        role_id: role_id.get_untracked(),
                        name: name.get_untracked(),
                        permissions: permissions.get_untracked().split(',').map(str::trim).filter(|value| !value.is_empty()).map(ToOwned::to_owned).collect(),
                    });
                }>"Save role"</button>
                <Show when=move || upsert_action.value().get().is_some()><p class="result-line">{move || action_result_text(upsert_action.value().get())}</p></Show>
            </section>
        },
    )
}

#[island(lazy)]
fn OrganizationPermissionsPage() -> impl IntoView {
    let roles = browser_load(list_current_organization_roles);
    page_shell(
        "Permissions",
        "Inspect effective permission assignments through organization roles.",
        view! {
            <section class="panel">
                <div class="client-data-slot">
                    {move || match roles.get() {
                        Some(Ok(response)) => view! { <div class="action-stack"><For each=move || response.roles.clone() key=|role| role.role_id.clone() children=move |role| view! {
                            <article class="compact-panel"><h3>{role.name}</h3><p class="result-line">{role.permissions.join(", ")}</p></article>
                        } /></div> }.into_any(),
                        Some(Err(error)) => view! { <p class="error-banner">{server_error_text(error)}</p> }.into_any(),
                        None => view! { <p class="result-line">"Loading permissions"</p> }.into_any(),
                    }}
                </div>
            </section>
        },
    )
}

#[island(lazy)]
fn OrganizationAuditPage() -> impl IntoView {
    let audit = browser_load(list_current_organization_audit);
    page_shell(
        "Audit activity",
        "Cursor-based audit reads share the same authorization path as the gRPC server stream.",
        view! {
            <section class="panel"><div class="client-data-slot">
                {move || match audit.get() {
                    Some(Ok(response)) => view! { <dl class="kv"><For each=move || response.events.clone() key=|event| event.sequence children=move |event| view! {
                        <dt>{event.action}</dt><dd>{format!("{}:{} / {}", event.target_type, event.target_id, event.outcome)}</dd>
                    } /></dl> }.into_any(),
                    Some(Err(error)) => view! { <p class="error-banner">{server_error_text(error)}</p> }.into_any(),
                    None => view! { <p class="result-line">"Loading audit activity"</p> }.into_any(),
                }}
            </div></section>
        },
    )
}

#[component]
fn OrganizationLinks() -> impl IntoView {
    view! {
        <section class="panel"><h2>"Organization management"</h2><div class="actions">
            <a class="link-button" href="/organizations/members">"Members"</a>
            <a class="link-button" href="/organizations/invitations">"Invitations"</a>
            <a class="link-button" href="/organizations/roles">"Roles"</a>
            <a class="link-button" href="/organizations/permissions">"Permissions"</a>
            <a class="link-button" href="/organizations/audit">"Audit"</a>
        </div></section>
    }
}

#[island(lazy)]
fn AdminUsersPage() -> impl IntoView {
    let users = browser_load(list_admin_users);
    page_shell(
        "System users",
        "Disable or recover users without deleting immutable audit history.",
        view! { <section class="panel"><div class="client-data-slot">
            {move || match users.get() {
                Some(Ok(response)) => view! { <dl class="kv"><For each=move || response.users.clone() key=|user| user.user_id.clone() children=move |user| view! {
                    <dt>{user.primary_email}</dt><dd>{if user.disabled { "disabled" } else if user.email_verified { "active / verified" } else { "pending verification" }}</dd>
                } /></dl> }.into_any(),
                Some(Err(error)) => view! { <p class="error-banner">{server_error_text(error)}</p> }.into_any(),
                None => view! { <p class="result-line">"Loading users"</p> }.into_any(),
            }}
        </div></section> },
    )
}

#[island(lazy)]
fn AdminHealthPage() -> impl IntoView {
    let health = browser_load(get_admin_health);
    page_shell(
        "Configuration health",
        "Verify the active storage, mail, and authorization profile.",
        view! { <section class="panel"><div class="client-data-slot">
            {move || match health.get() {
                Some(Ok(value)) => view! { <dl class="kv">
                    <dt>"Status"</dt><dd>{value.status}</dd>
                    <dt>"Storage"</dt><dd>{value.storage_backend}</dd>
                    <dt>"Mail"</dt><dd>{value.mail_transport}</dd>
                    <dt>"Authorization"</dt><dd>{value.authorization_provider}</dd>
                </dl> }.into_any(),
                Some(Err(error)) => view! { <p class="error-banner">{server_error_text(error)}</p> }.into_any(),
                None => view! { <p class="result-line">"Loading health"</p> }.into_any(),
            }}
        </div></section> },
    )
}

#[island(lazy)]
fn AdminPoliciesPage() -> impl IntoView {
    let versions = browser_load(list_policy_versions);
    let publish_action = ServerAction::<PublishPolicyVersion>::new();
    let (policy_text, set_policy_text) = signal(String::new());
    let (schema_text, set_schema_text) = signal(String::new());
    page_shell(
        "Cedar policy versions",
        "Validate and publish a versioned policy bundle with MFA step-up.",
        view! {
            <section class="panel"><h2>"Published versions"</h2><div class="client-data-slot">
                {move || match versions.get() {
                    Some(Ok(response)) => view! { <dl class="kv"><For each=move || response.versions.clone() key=|version| version.version_id.clone() children=move |version| view! {
                        <dt>{version.version_id}</dt><dd>{format!("{} / {}", version.status, version.policy_hash)}</dd>
                    } /></dl> }.into_any(),
                    Some(Err(error)) => view! { <p class="error-banner">{server_error_text(error)}</p> }.into_any(),
                    None => view! { <p class="result-line">"Loading versions"</p> }.into_any(),
                }}
            </div></section>
            <section class="panel"><h2>"Publish candidate"</h2>
                <label><span>"Cedar policy"</span><textarea prop:value=move || policy_text.get() on:input=move |event| set_policy_text.set(event_target_value(&event)) /></label>
                <label><span>"Cedar schema JSON"</span><textarea prop:value=move || schema_text.get() on:input=move |event| set_schema_text.set(event_target_value(&event)) /></label>
                <button type="button" class="primary-button" on:click=move |_| {
                    publish_action.dispatch(PublishPolicyVersion {
                        policy_text: policy_text.get_untracked(),
                        schema_text: schema_text.get_untracked(),
                    });
                }>"Validate and publish"</button>
                <Show when=move || publish_action.value().get().is_some()><p class="result-line">{move || action_result_text(publish_action.value().get())}</p></Show>
            </section>
        },
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

#[island]
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
    let capture_enabled = browser_load(development_mail_capture_enabled);
    let capture_action = ServerAction::<LatestDevelopmentMail>::new();
    let capture_pending = capture_action.pending();
    let capture_value = capture_action.value();
    let registration_complete = RwSignal::new(false);

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
            redirect_browser(&message.body_text);
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

            <Show when=move || registration_complete.get()>
                <div class="auth-success">
                    <p><strong>"Account created."</strong> " Check your inbox for the one-time verification link."</p>
                    <Show when=move || matches!(capture_enabled.get(), Some(Ok(true)))>
                        <p>"Capture mode does not send internet email. The local worker stores the message for this example."</p>
                        <button
                            type="button"
                            class="auth-secondary"
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
                            <p class="auth-inline-error">
                                {move || selected_action_error(capture_value.get()).unwrap_or_default()}
                            </p>
                        </Show>
                    </Show>
                </div>
            </Show>

            <div class="auth-mode-switch" role="tablist" aria-label="Authentication mode" hidden=move || registration_complete.get()>
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

            <form class="auth-fields" hidden=move || registration_complete.get() on:submit=move |event| {
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
                        "Use 15 to 128 characters. Only a derived password hash is stored."
                    } else {
                        "Your session is issued by the local Spin auth service."
                    }}</small>
                </label>

                <p
                    class="auth-error"
                    hidden=move || client_error.get().is_none()
                >
                    {move || client_error.get().unwrap_or_default()}
                </p>
                <p
                    class="auth-error"
                    hidden=move || selected_auth_error(
                        register_mode.get(),
                        login_value.get(),
                        register_value.get(),
                    ).is_none()
                >
                    {move || selected_auth_error(
                        register_mode.get(),
                        login_value.get(),
                        register_value.get(),
                    ).unwrap_or_default()}
                </p>

                <button
                    type="submit"
                    class="auth-submit"
                    disabled=move || login_pending.get() || register_pending.get()
                    aria-busy=move || if login_pending.get() || register_pending.get() { "true" } else { "false" }
                >
                    <span
                        class="auth-button-spinner"
                        aria-hidden="true"
                        hidden=move || !(login_pending.get() || register_pending.get())
                    ></span>
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

#[island]
fn EmailVerificationForm() -> impl IntoView {
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
        <section class="auth-form">
            <div>
                <p class="auth-kicker">"Email verification"</p>
                <h1 class="auth-title">"Verify your email"</h1>
                <p class="auth-copy">"The one-time link is hashed at rest and can be used once."</p>
            </div>
            <Show when=move || pending.get()>
                <p class="result-line">"Verifying email"</p>
            </Show>
            <Show when=move || selected_action_error(value.get()).is_some()>
                <p class="auth-error">{move || selected_action_error(value.get()).unwrap_or_default()}</p>
            </Show>
            <Show when=move || one_time_token_from_url().is_none()>
                <p class="auth-notice">"Open this page from the one-time link in your verification message."</p>
            </Show>
            <a class="auth-text-link" href="/verify-email/resend">"Request another message"</a>
        </section>
    }
}

#[island]
fn ResendVerificationForm() -> impl IntoView {
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
            redirect_browser(&message.body_text);
        }
    });

    view! {
        <section class="auth-form">
            <div>
                <p class="auth-kicker">"Email verification"</p>
                <h1 class="auth-title">"Send a fresh link"</h1>
                <p class="auth-copy">"The response is generic whether or not the account exists."</p>
            </div>
            <form class="auth-fields" on:submit=move |event| {
                event.prevent_default();
                action.dispatch(ResendEmailVerification {
                    email: email.get_untracked(),
                    redirect_url: Some("/dashboard".to_string()),
                });
            }>
                <label class="auth-field">
                    <span>"Email"</span>
                    <input
                        class="auth-input"
                        type="email"
                        autocomplete="email"
                        prop:value=move || email.get()
                        on:input=move |event| set_email.set(event_target_value(&event))
                    />
                </label>
                <button type="submit" class="auth-submit" disabled=move || pending.get()>
                    "Send verification link"
                </button>
                <Show when=move || value.get().is_some()>
                    <p class="result-line">{move || action_result_text(value.get())}</p>
                </Show>
                <Show when=move || value.get().is_some() && matches!(capture_enabled.get(), Some(Ok(true)))>
                    <p class="auth-notice">"Capture mode stores this message locally; it will not arrive in an external inbox."</p>
                    <button
                        type="button"
                        class="auth-secondary"
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
                        <p class="auth-inline-error">{move || selected_action_error(capture_value.get()).unwrap_or_default()}</p>
                    </Show>
                </Show>
            </form>
        </section>
    }
}

#[island]
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
                    "Enter your email and we will send reset instructions if an account exists."
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
                <p
                    class="auth-error"
                    hidden=move || client_error.get().is_none()
                >
                    {move || client_error.get().unwrap_or_default()}
                </p>
                <div hidden=move || value.get().is_none()>
                    <PasswordResetStartResult result=move || value.get() />
                </div>
                <button
                    type="submit"
                    class="auth-submit"
                    disabled=move || pending.get()
                    aria-busy=move || if pending.get() { "true" } else { "false" }
                >
                    <span
                        class="auth-button-spinner"
                        aria-hidden="true"
                        hidden=move || !pending.get()
                    ></span>
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
                let _ = response;
                view! {
                    <div class="auth-success">
                        <p>"If an account exists, reset instructions are ready to send."</p>
                    </div>
                }.into_any()
            }
            Some(Err(error)) => view! { <p class="auth-error">{server_error_text(error)}</p> }.into_any(),
            None => view! {}.into_any(),
        }}
    }
}

#[island]
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
                    <small>"Use 15 to 128 characters. Existing sessions should be reviewed after reset."</small>
                </label>
                <p
                    class="auth-error"
                    hidden=move || client_error.get().is_none()
                >
                    {move || client_error.get().unwrap_or_default()}
                </p>
                <p
                    class="auth-error"
                    hidden=move || selected_action_error(value.get()).is_none()
                >
                    {move || selected_action_error(value.get()).unwrap_or_default()}
                </p>
                <button
                    type="submit"
                    class="auth-submit"
                    disabled=move || pending.get()
                    aria-busy=move || if pending.get() { "true" } else { "false" }
                >
                    <span
                        class="auth-button-spinner"
                        aria-hidden="true"
                        hidden=move || !pending.get()
                    ></span>
                    <span>{move || if pending.get() { "Updating password" } else { "Reset password" }}</span>
                </button>
                <a class="auth-text-link" href="/login">"Return to sign in"</a>
            </form>
        </section>
    }
}

#[island]
fn OptionalLoginMethods() -> impl IntoView {
    let capabilities = browser_load(get_auth_capabilities);

    view! {
        <section
            class="optional-methods"
            hidden=move || !capabilities.get().is_some_and(|result| {
                result.is_ok_and(|capabilities| {
                    (capabilities.oauth_enabled && !capabilities.providers.is_empty())
                        || capabilities.passkeys_enabled
                })
            })
        >
            <div
                class="client-data-slot"
                hidden=move || !capabilities.get().is_some_and(|result| {
                    result.is_ok_and(|capabilities| {
                        capabilities.oauth_enabled && !capabilities.providers.is_empty()
                    })
                })
            >
                <OAuthProviderList />
            </div>
            <div
                class="client-data-slot"
                hidden=move || !capabilities.get().is_some_and(|result| {
                    result.is_ok_and(|capabilities| capabilities.passkeys_enabled)
                })
            >
                <PasskeyLoginForm />
            </div>
        </section>
    }
}

#[component]
fn OAuthProviderList() -> impl IntoView {
    let providers = browser_load(list_auth_providers);

    view! {
        <section class="panel compact-panel">
            <h2>"Connected providers"</h2>
            <p
                class="result-line"
                hidden=move || matches!(providers.get(), Some(Ok(providers)) if !providers.is_empty())
            >
                {move || match providers.get() {
                    None => "Loading providers".to_string(),
                    Some(Ok(providers)) if providers.is_empty() => {
                        "No providers are enabled.".to_string()
                    }
                    Some(Ok(_)) => String::new(),
                    Some(Err(error)) => server_error_text(error),
                }}
            </p>
            <div
                class="button-grid"
                hidden=move || !matches!(providers.get(), Some(Ok(providers)) if !providers.is_empty())
            >
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

                let verify_action = verify_action;
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

#[island(lazy)]
fn OptionalPasskeyRegistration() -> impl IntoView {
    let capabilities = browser_load(get_auth_capabilities);

    view! {
        <div class="client-data-slot">
            {move || match capabilities.get() {
                Some(Ok(capabilities)) if capabilities.passkeys_enabled => {
                    view! { <PasskeyRegistrationForm /> }.into_any()
                }
                _ => view! {}.into_any(),
            }}
        </div>
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

#[island]
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

#[island]
fn SessionSummary() -> impl IntoView {
    let session = browser_load(get_current_session);

    view! {
        <section class="panel">
            <h2>"Current session"</h2>
            <dl
                class="kv"
                hidden=move || !matches!(session.get(), Some(Ok(view)) if view.authenticated)
            >
                <dt>"Tenant"</dt>
                <dd>{move || session.get().and_then(Result::ok).and_then(|view| view.tenant_id).unwrap_or_default()}</dd>
                <dt>"User"</dt>
                <dd>{move || session.get().and_then(Result::ok).and_then(|view| view.user_id).unwrap_or_default()}</dd>
                <dt>"Email"</dt>
                <dd>{move || session.get().and_then(Result::ok).and_then(|view| view.primary_email).unwrap_or_default()}</dd>
            </dl>
            <p
                class="result-line"
                hidden=move || matches!(session.get(), Some(Ok(view)) if view.authenticated)
            >
                {move || match session.get() {
                    None => "Loading".to_string(),
                    Some(Ok(_)) => "No active session".to_string(),
                    Some(Err(error)) => error.to_string(),
                }}
            </p>
        </section>
    }
}

#[island(lazy)]
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

#[island(lazy)]
fn SigningKeyRotationForm() -> impl IntoView {
    let rotate_action = ServerAction::<RotateSigningKey>::new();
    let pending = rotate_action.pending();
    let value = rotate_action.value();
    let (kid, set_kid) = signal("fullstack-app-next-hs256".to_string());
    let (retire_previous, set_retire_previous) = signal(true);
    let keys = browser_load(list_signing_keys);

    let submit = move |_| {
        rotate_action.dispatch(RotateSigningKey {
            kid: kid.get_untracked(),
            retire_previous: retire_previous.get_untracked(),
        });
    };

    view! {
        <section class="panel">
            <h2>"Signing key rotation"</h2>
            <p class="muted">"Requires a system-administrator session with MFA step-up."</p>
            <div class="client-data-slot">
                {move || match keys.get() {
                    Some(Ok(response)) if response.keys.is_empty() => view! {
                        <p class="result-line">"No signing keys are configured."</p>
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
            </div>
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

#[island(lazy)]
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

#[island]
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
                <a href="/">"Home"</a>
                <a href="/dashboard">"Dashboard"</a>
                <a href="/organizations">"Organizations"</a>
                <a href="/account/profile">"Account"</a>
                <a href="/admin/health">"System"</a>
            </nav>
            <section class="page-header">
                <p>"wasi-auth / ddd_cqrs_es"</p>
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
        <div class="error-page">
            <section class="error-card">
                <AuthBrand />
                <p class="auth-kicker">"Request interrupted"</p>
                <h1 class="error-title">{title}</h1>
                <p class="error-copy">{subtitle}</p>
                <div class="error-actions">{children}</div>
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
    if register_mode && !(15..=128).contains(&password.chars().count()) {
        return Err("Password must contain 15 to 128 characters.".to_string());
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

fn browser_load<T, Fut, F>(load: F) -> ReadSignal<Option<T>>
where
    T: Send + Sync + 'static,
    Fut: std::future::Future<Output = T> + 'static,
    F: FnOnce() -> Fut + Send + Sync + 'static,
{
    let (value, set_value) = signal(None);

    #[cfg(feature = "hydrate")]
    {
        let load = StoredValue::new(Some(load));
        Effect::new(move |_| {
            if let Some(load) = load.write_value().take() {
                spawn_local(async move {
                    let _ = after_island_hydration().await;
                    set_value.set(Some(load().await));
                });
            }
        });
    }

    #[cfg(not(feature = "hydrate"))]
    let _ = (set_value, load);

    value
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

fn one_time_token_from_url() -> Option<String> {
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

#[cfg_attr(feature = "hydrate", allow(dead_code))]
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
fn server_fn_request_auth() -> crate::application::RequestAuth {
    if let Ok(context) = wasi_auth::leptos::current_verified_request_context() {
        return crate::application::RequestAuth::from_verified(context);
    }
    crate::application::RequestAuth::from_parts(current_session_id_from_cookie(), None, None)
}

#[cfg(feature = "ssr")]
fn session_id_from_cookie_header(cookie_header: &str) -> Option<String> {
    cookie_header.split(';').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        if matches!(name, "__Host-session" | "wasi_auth_dev_session") && !value.trim().is_empty() {
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
pub async fn complete_email_verification(
    token: String,
    redirect_url: Option<String>,
) -> Result<LoginCompletionResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response =
            crate::application::complete_email_verification(EmailVerificationCompleteRequest {
                token,
                redirect_url,
            })
            .await
            .map_err(server_fn_error)?;
        set_session_cookie(&response).await;
        Ok(browser_login_response(response))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (token, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn resend_email_verification(
    email: String,
    redirect_url: Option<String>,
) -> Result<AcceptedResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::resend_email_verification(EmailVerificationResendRequest {
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
pub async fn development_mail_capture_enabled() -> Result<bool, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        Ok(crate::auth_product::development_mail_capture_enabled().await)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn latest_development_mail(
    recipient: String,
    message_kind: String,
) -> Result<CapturedMailResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::latest_captured_mail(recipient, message_kind)
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (recipient, message_kind);
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
        crate::application::start_passkey_registration(
            PasskeyStartRequest {
                email,
                redirect_url,
            },
            server_fn_request_auth(),
        )
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
        let response = crate::application::verify_passkey_registration(
            PasskeyVerifyRequest {
                challenge_id,
                credential_json,
                redirect_url,
            },
            server_fn_request_auth(),
        )
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
pub async fn change_password(
    current_password: String,
    new_password: String,
) -> Result<AcceptedResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::change_password(
            PasswordChangeRequest {
                current_password,
                new_password,
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (current_password, new_password);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_account_sessions() -> Result<AccountSessionListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_sessions(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn revoke_account_session(session_id: String) -> Result<AcceptedResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let current_session = current_session_id_from_cookie();
        let response = crate::application::revoke_account_session(
            SessionRevokeRequest {
                session_id: session_id.clone(),
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)?;
        if current_session.as_deref() == Some(session_id.as_str()) {
            clear_session_cookie().await;
        }
        Ok(response)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = session_id;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn get_mfa_status() -> Result<MfaStatusResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::mfa_status(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn start_totp_enrollment() -> Result<MfaEnrollStartResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::start_totp_enrollment(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn confirm_totp_enrollment(
    code: String,
) -> Result<MfaEnrollConfirmResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::confirm_totp_enrollment(
            MfaCodeRequest { code },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = code;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn verify_totp_step_up(code: String) -> Result<SessionView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::verify_totp_step_up(MfaCodeRequest { code }, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = code;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn verify_recovery_code(code: String) -> Result<SessionView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::use_recovery_code_for_step_up(
            MfaCodeRequest { code },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = code;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn save_auth_provider(
    provider_id: String,
    enabled: bool,
) -> Result<AuthProviderSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::admin_save_provider(provider_id, enabled, server_fn_request_auth())
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
        crate::application::save_redirect_allowlist(redirects_json, server_fn_request_auth())
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
pub async fn list_signing_keys() -> Result<SigningKeyListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_signing_keys(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn rotate_signing_key(
    kid: String,
    retire_previous: bool,
) -> Result<SigningKeyRotateResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::rotate_signing_key(
            SigningKeyRotateRequest {
                kid,
                retire_previous: Some(retire_previous),
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (kid, retire_previous);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn get_authorization_capabilities()
-> Result<AuthorizationCapabilitiesResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::authorization_capabilities()
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn list_organizations() -> Result<OrganizationListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_organizations(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn create_organization(
    name: String,
) -> Result<crate::contracts::OrganizationSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::create_organization(
            OrganizationCreateRequest { name },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = name;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn select_organization(organization_id: String) -> Result<SessionView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::select_organization(
            crate::contracts::OrganizationSelectRequest { organization_id },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = organization_id;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_current_organization_members() -> Result<MembershipListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let organization_id = current_organization_id().await?;
        crate::application::list_members(organization_id, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn list_current_organization_invitations() -> Result<InvitationListResponse, ServerFnError>
{
    #[cfg(feature = "ssr")]
    {
        let organization_id = current_organization_id().await?;
        crate::application::list_invitations(organization_id, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn invite_current_organization_member(
    email: String,
    role_id: String,
) -> Result<crate::contracts::InvitationSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let organization_id = current_organization_id().await?;
        crate::application::invite_member(
            InvitationCreateRequest {
                organization_id,
                email,
                role_id,
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (email, role_id);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_current_organization_roles() -> Result<RoleListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let organization_id = current_organization_id().await?;
        crate::application::list_roles(organization_id, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn upsert_current_organization_role(
    role_id: String,
    name: String,
    permissions: Vec<String>,
) -> Result<crate::contracts::RoleSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let organization_id = current_organization_id().await?;
        crate::application::upsert_role(
            RoleUpsertRequest {
                organization_id,
                role_id,
                name,
                permissions,
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (role_id, name, permissions);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_current_organization_audit() -> Result<AuditEventListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let organization_id = current_organization_id().await?;
        crate::application::list_audit_events(
            Some(organization_id),
            0,
            100,
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn list_admin_users() -> Result<AdminUserListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_admin_users(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn get_admin_health() -> Result<HealthStatusResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::get_health(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn list_policy_versions() -> Result<PolicyVersionListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_policy_versions(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn publish_policy_version(
    policy_text: String,
    schema_text: String,
) -> Result<crate::contracts::PolicyVersionSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::publish_policy(
            PolicyPublishRequest {
                policy_text,
                schema_text,
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (policy_text, schema_text);
        unreachable!()
    }
}

#[cfg(feature = "ssr")]
async fn current_organization_id() -> Result<String, ServerFnError> {
    let session =
        crate::application::require_authenticated_route_for(current_session_id_from_cookie())
            .await
            .map_err(server_fn_error)?;
    session
        .tenant_id
        .filter(|organization_id| organization_id != "tenant:default")
        .ok_or_else(|| ServerFnError::ServerError("select an organization first".to_owned()))
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
