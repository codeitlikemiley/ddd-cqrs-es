//! Thin auth route pages (shells wrapping forms/islands).

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use super::forms::{
    EmailPasswordAuthForm, EmailVerificationForm, ForgotPasswordForm, InvitationAcceptForm,
    LogoutForm, OAuthCallbackStatus, OAuthProviderList, OptionalPasskeyRegistration,
    ResendVerificationForm, ResetPasswordForm,
};
use crate::app::helpers::{next_url, percent_encode_component, redirect_browser, set_page_status};
use crate::app::{browser_load, get_current_session};
use crate::ui::classes::{
    AUTH_CARD, AUTH_PAGE, BANNER_SUCCESS, BTN_SECONDARY, BUTTON_ROW, CLIENT_DATA_SLOT, SECTION_LABEL,
};
use crate::ui::{AuthBrand, error_page_shell, page_shell};
use leptos::prelude::*;
use leptos_meta::*;

#[component]
pub fn LoginPage() -> impl IntoView {
    view! {
        <div class=AUTH_PAGE data-testid="auth-page">
            <ExistingSessionRedirect />
            <section class=AUTH_CARD data-testid="auth-card">
                <AuthBrand />
                <EmailPasswordAuthForm register_default=false />
            </section>
        </div>
    }
}

#[component]
pub fn RegisterPage() -> impl IntoView {
    view! {
        <div class=AUTH_PAGE data-testid="auth-page">
            <ExistingSessionRedirect />
            <section class=AUTH_CARD data-testid="auth-card">
                <AuthBrand />
                <EmailPasswordAuthForm register_default=true />
            </section>
        </div>
    }
}

#[component]
pub fn ForgotPasswordPage() -> impl IntoView {
    view! {
        <div class=AUTH_PAGE data-testid="auth-page">
            <ExistingSessionRedirect />
            <section class=AUTH_CARD data-testid="auth-card">
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
        <div class=AUTH_PAGE data-testid="auth-page">
            <section class=AUTH_CARD data-testid="auth-card">
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
        <div class=AUTH_PAGE data-testid="auth-page">
            <section class=AUTH_CARD data-testid="auth-card">
                <AuthBrand />
                <InvitationAcceptForm />
            </section>
        </div>
    }
}

#[component]
pub fn VerifyEmailPage() -> impl IntoView {
    view! {
        <div class=AUTH_PAGE data-testid="auth-page">
            <section class=AUTH_CARD data-testid="auth-card">
                <AuthBrand />
                <EmailVerificationForm />
            </section>
        </div>
    }
}

#[component]
pub fn VerificationPendingPage() -> impl IntoView {
    view! {
        <div class=AUTH_PAGE data-testid="auth-page">
            <section class=AUTH_CARD data-testid="auth-card">
                <AuthBrand />
                <section class="grid gap-7">
                    <div>
                        <p class=SECTION_LABEL>"Email verification"</p>
                        <h1 class="mt-3 mb-0 text-[32px] font-semibold leading-[1.05] tracking-tight">"Check your inbox"</h1>
                        <p class="mt-3 mb-0 max-w-[34ch] text-[15px] leading-relaxed text-secondary">
                            "Your account is pending. Open the one-time verification link before signing in."
                        </p>
                    </div>
                    <p class=BANNER_SUCCESS>
                        "Local capture mode keeps messages on this machine. Start the app with `make dev` to run delivery automatically."
                    </p>
                    <a class="inline-flex justify-center no-underline text-primary" href="/verify-email/resend">"Send another verification link"</a>
                </section>
            </section>
        </div>
    }
}

#[component]
pub fn ResendVerificationPage() -> impl IntoView {
    view! {
        <div class=AUTH_PAGE data-testid="auth-page">
            <section class=AUTH_CARD data-testid="auth-card">
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
        <div class=CLIENT_DATA_SLOT>
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
            <div class=BUTTON_ROW>
                <a class=BTN_SECONDARY href="/account/sessions">"Sessions"</a>
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
            class=BTN_SECONDARY
            href=move || format!("/login?next={}", percent_encode_component(&next_url()))
        >
            "Sign in"
        </a>
    }
}

#[component]
pub fn ReturnToLoginLink() -> impl IntoView {
    view! { <a class=BTN_SECONDARY href="/login">"Return to sign in"</a> }
}
