use leptos::prelude::*;

use super::brand::AuthBrand;
use super::classes::{
    ACCOUNT_PAGE, ACCOUNT_PAGE_BODY, ACCOUNT_PAGE_HEADER, ACCOUNT_PAGE_SUBTITLE,
    ACCOUNT_PAGE_TITLE, ERROR_ACTIONS, ERROR_CARD, ERROR_COPY, ERROR_PAGE, ERROR_TITLE, PAGE_GRID,
    SECTION_LABEL, WORKSPACE_PAGE_HEADER, WORKSPACE_PAGE_SUBTITLE, WORKSPACE_PAGE_TITLE,
};

/// Wide workspace page header + grid (orgs, admin, dashboard-adjacent).
pub fn page_shell(
    title: &'static str,
    subtitle: &'static str,
    children: impl IntoView + 'static,
) -> impl IntoView {
    view! {
        <section class=WORKSPACE_PAGE_HEADER>
            <h1 class=WORKSPACE_PAGE_TITLE>{title}</h1>
            <span class=WORKSPACE_PAGE_SUBTITLE>{subtitle}</span>
        </section>
        <section class=PAGE_GRID>
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
        <div class=ACCOUNT_PAGE data-testid="account-page">
            <header class=ACCOUNT_PAGE_HEADER>
                <h1 class=ACCOUNT_PAGE_TITLE>{title}</h1>
                <p class=ACCOUNT_PAGE_SUBTITLE>{subtitle}</p>
            </header>
            <div class=ACCOUNT_PAGE_BODY>
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
        <div class="mx-auto box-border min-h-dvh w-full max-w-[1120px] px-4 pb-12 text-primary">
            <header class="flex items-center justify-between gap-4 border-b border-border-subtle py-4">
                <a class="inline-flex min-w-0 items-center gap-2.5 no-underline" href="/" aria-label="wasi-auth home">
                    <span class="inline-flex h-8 w-8 flex-none items-center justify-center rounded-[10px] bg-inverse text-sm font-bold text-on-inverse" aria-hidden="true">"d"</span>
                    <span>
                        <strong class="block text-[13px] font-semibold leading-tight tracking-tight">"wasi-auth"</strong>
                        <small class="mt-0.5 block text-[11px] leading-snug text-tertiary">"ddd_cqrs_es fullstack"</small>
                    </span>
                </a>
                <span class="inline-flex items-center gap-2 text-xs text-secondary">
                    <span class="inline-block h-1.5 w-1.5 rounded-full bg-success" aria-hidden="true"></span>
                    "Spin runtime"
                </span>
            </header>
            <section class="border-b border-border-subtle pb-[30px] pt-[72px]">
                <p class=SECTION_LABEL>"wasi-auth / ddd_cqrs_es"</p>
                <h1 class="my-3 max-w-[18ch] text-[clamp(32px,5vw,48px)] font-semibold leading-tight tracking-tight">{title}</h1>
                <span class="text-secondary">{subtitle}</span>
            </section>
            <section class=PAGE_GRID>
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
        <div class=ERROR_PAGE data-testid="error-page">
            <section class=ERROR_CARD>
                <AuthBrand />
                <p class=SECTION_LABEL>"Request interrupted"</p>
                <h1 class=ERROR_TITLE>{title}</h1>
                <p class=ERROR_COPY>{subtitle}</p>
                <div class=ERROR_ACTIONS>{children}</div>
            </section>
        </div>
    }
}
