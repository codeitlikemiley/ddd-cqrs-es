//! Thin auth route pages (shells wrapping forms/islands).

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use super::forms::{
    EmailPasswordAuthForm, EmailVerificationForm, ForgotPasswordForm, InvitationAcceptForm,
    LogoutForm, OAuthCallbackStatus, OAuthProviderList, OptionalPasskeyRegistration,
    ResendVerificationForm, ResetPasswordForm,
};
use crate::app::helpers::{
    next_url, percent_encode_component, redirect_browser, set_page_status,
};
use crate::app::{browser_load, get_current_session};
use crate::ui::{error_page_shell, page_shell, AuthBrand};
use leptos::prelude::*;
use leptos_meta::*;


#[component]
pub fn LoginPage() -> impl IntoView {
    view! {
        <div class="auth-page">
            <ExistingSessionRedirect />
            <section class="auth-card">
                <AuthBrand />
                <EmailPasswordAuthForm register_default=false />
            </section>
        </div>
    }
}

#[component]
pub fn RegisterPage() -> impl IntoView {
    view! {
        <div class="auth-page">
            <ExistingSessionRedirect />
            <section class="auth-card">
                <AuthBrand />
                <EmailPasswordAuthForm register_default=true />
            </section>
        </div>
    }
}

#[component]
pub fn ForgotPasswordPage() -> impl IntoView {
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
pub fn ResetPasswordPage() -> impl IntoView {
    // Do not mount ExistingSessionRedirect here. Tokenized reset links must
    // render the form even when a stale session cookie is still present.
    view! {
        <div class="auth-page">
            <section class="auth-card">
                <AuthBrand />
                <ResetPasswordForm />
            </section>
        </div>
    }
}

#[component]
pub fn InvitationAcceptPage() -> impl IntoView {
    // Authenticated document shell; unauthenticated browsers are redirected by
    // protected_ui_redirect with next= preserving ?token=.
    view! {
        <div class="auth-page">
            <section class="auth-card">
                <AuthBrand />
                <InvitationAcceptForm />
            </section>
        </div>
    }
}

#[component]
pub fn VerifyEmailPage() -> impl IntoView {
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
pub fn VerificationPendingPage() -> impl IntoView {
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
pub fn ResendVerificationPage() -> impl IntoView {
    view! {
        <div class="auth-page">
            <section class="auth-card">
                <AuthBrand />
                <ResendVerificationForm />
            </section>
        </div>
    }
}

#[island]
pub fn ExistingSessionRedirect() -> impl IntoView {
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
pub fn OAuthCallbackPage() -> impl IntoView {
    page_shell(
        "Completing sign-in",
        "The provider callback will be verified by the server.",
        view! { <OAuthCallbackStatus /> },
    )
}

#[component]
pub fn OAuthCallbackErrorPage() -> impl IntoView {
    set_page_status(http::StatusCode::BAD_REQUEST);
    error_page_shell(
        "Sign-in failed",
        "The provider response could not be accepted.",
        view! { <ReturnToLoginLink /> },
    )
}

#[component]
pub fn AuthRequiredPage() -> impl IntoView {
    set_page_status(http::StatusCode::UNAUTHORIZED);
    error_page_shell(
        "Authentication required",
        "Sign in before continuing.",
        view! { <LoginRedirectLink /> },
    )
}

#[component]
pub fn ForbiddenPage() -> impl IntoView {
    set_page_status(http::StatusCode::FORBIDDEN);
    error_page_shell(
        "Access denied",
        "The current account cannot open this page.",
        view! {
            <div class="actions">
                <a class="link-button" href="/account/sessions">"Sessions"</a>
                <LogoutForm />
            </div>
        },
    )
}

#[component]
pub fn SessionExpiredPage() -> impl IntoView {
    set_page_status(http::StatusCode::UNAUTHORIZED);
    error_page_shell(
        "Session expired",
        "Sign in again to continue.",
        view! { <LoginRedirectLink /> },
    )
}

#[island(lazy)]
pub fn PasskeyUnsupportedPage() -> impl IntoView {
    error_page_shell(
        "Passkey unavailable",
        "Use email and password or an enabled provider.",
        view! { <OAuthProviderList /> },
    )
}

#[component]
pub fn LoginRedirectLink() -> impl IntoView {
    view! {
        <a
            class="link-button"
            href=move || format!("/login?next={}", percent_encode_component(&next_url()))
        >
            "Sign in"
        </a>
    }
}

#[component]
pub fn ReturnToLoginLink() -> impl IntoView {
    view! { <a class="link-button" href="/login">"Return to sign in"</a> }
}
