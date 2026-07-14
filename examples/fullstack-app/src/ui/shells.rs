use leptos::prelude::*;

use super::brand::AuthBrand;

/// Wide workspace page header + grid (orgs, admin, dashboard-adjacent).
pub fn page_shell(
    title: &'static str,
    subtitle: &'static str,
    children: impl IntoView + 'static,
) -> impl IntoView {
    view! {
        <section class="page-header workspace-page-header">
            <h1>{title}</h1>
            <span class="workspace-page-subtitle">{subtitle}</span>
        </section>
        <section class="page-grid">
            {children}
        </section>
    }
}

/// Narrow centered column for account + vault settings (~640px).
pub fn account_page_shell(
    title: &'static str,
    subtitle: &'static str,
    _active: &'static str,
    children: impl IntoView + 'static,
) -> impl IntoView {
    view! {
        <div class="account-page">
            <header class="account-page-header">
                <h1>{title}</h1>
                <p class="account-page-subtitle">{subtitle}</p>
            </header>
            <div class="account-page-body">
                {children}
            </div>
        </div>
    }
}

/// Marketing / public page chrome.
#[allow(dead_code)]
pub fn public_page_shell(
    title: &'static str,
    subtitle: &'static str,
    children: impl IntoView + 'static,
) -> impl IntoView {
    view! {
        <div class="page">
            <header class="page-brand">
                <a class="page-brand-link" href="/" aria-label="wasi-auth home">
                    <span class="page-brand-mark" aria-hidden="true">"d"</span>
                    <span>
                        <strong>"wasi-auth"</strong>
                        <small>"ddd_cqrs_es fullstack"</small>
                    </span>
                </a>
                <span class="page-brand-status">
                    <span class="status-dot" aria-hidden="true"></span>
                    "Spin runtime"
                </span>
            </header>
            <section class="page-header">
                <p class="page-header-kicker">"wasi-auth / ddd_cqrs_es"</p>
                <h1>{title}</h1>
                <span>{subtitle}</span>
            </section>
            <section class="page-grid">
                {children}
            </section>
        </div>
    }
}

/// Auth interrupt / error card shell.
pub fn error_page_shell(
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
