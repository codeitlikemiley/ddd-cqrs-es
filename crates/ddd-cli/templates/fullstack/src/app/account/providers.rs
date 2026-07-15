#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use crate::app::helpers::server_error_text;
use crate::app::{ListAuthProviders, browser_load, list_auth_providers};
use crate::contracts::AuthProviderSummary;
use crate::ui::account_page_shell;
use leptos::prelude::*;
use server_fn::ServerFnError;

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
