//! Admin / system console pages and forms.

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use crate::app::helpers::{action_result_text, server_error_text};
use crate::app::{
    GetAdminHealth, GetAuthorizationCapabilities, ListAdminUsers, ListPolicyVersions,
    ListSigningKeys, PublishPolicyVersion, RotateSigningKey, SaveAuthProvider,
    SaveRedirectAllowlist, browser_load, get_admin_health, get_authorization_capabilities,
    list_admin_users, list_policy_versions, list_signing_keys, publish_policy_version,
    rotate_signing_key, save_auth_provider, save_redirect_allowlist,
};
use crate::contracts::{
    AdminUserListResponse, AuthorizationCapabilitiesResponse, HealthStatusResponse,
    PolicyVersionListResponse, SigningKeyListResponse, SigningKeyRotateResponse,
};
use crate::ui::classes::{
    BANNER_ERROR, BTN_PRIMARY, BTN_SECONDARY, CLIENT_DATA_SLOT, FIELD, INLINE_FIELD, INPUT, KV_DD,
    KV_DT, KV_LIST, PANEL, PANEL_TITLE, RESULT_LINE, TEXTAREA,
};
use crate::ui::{ListSkeleton, page_shell};
use leptos::prelude::*;
use server_fn::ServerFnError;

#[component]
pub fn AuthProviderAdminPage() -> impl IntoView {
    page_shell(
        "Auth providers",
        "Configure OAuth and OIDC providers.",
        view! { <ProviderConfigForm /> },
    )
}

#[component]
pub fn SigningKeyAdminPage() -> impl IntoView {
    page_shell(
        "Signing keys",
        "Rotate the active access-token signing key.",
        view! { <SigningKeyRotationForm /> },
    )
}

#[component]
pub fn RedirectAllowlistPage() -> impl IntoView {
    page_shell(
        "Redirect allowlist",
        "Restrict browser redirect targets.",
        view! { <RedirectAllowlistForm /> },
    )
}

#[island(lazy)]
pub fn AuthorizationPolicyPage() -> impl IntoView {
    let capabilities = browser_load(get_authorization_capabilities);
    page_shell(
        "Authorization policy",
        "Inspect the active embedded Cedar provider. Policy publication is restricted to MFA-authenticated system administrators.",
        view! {
            <section class=PANEL>
                <h2 class=PANEL_TITLE>"Active provider"</h2>
                <div class=CLIENT_DATA_SLOT>
                    {move || capabilities.get().map(|result| match result {
                        Ok(value) => view! {
                            <dl class=KV_LIST>
                                <dt class=KV_DT>"Provider"</dt><dd class=KV_DD>{value.provider}</dd>
                                <dt class=KV_DT>"Maximum batch"</dt><dd class=KV_DD>{value.max_batch_checks}</dd>
                                <dt class=KV_DT>"Resource listing"</dt><dd class=KV_DD>{value.list_resources}</dd>
                                <dt class=KV_DT>"Consistency tokens"</dt><dd class=KV_DD>{value.consistency_tokens}</dd>
                            </dl>
                        }.into_any(),
                        Err(error) => view! {
                            <p class=RESULT_LINE>{server_error_text(error)}</p>
                        }.into_any(),
                    })}
                </div>
            </section>
        },
    )
}

#[island(lazy)]
pub fn AdminUsersPage() -> impl IntoView {
    let users = browser_load(list_admin_users);
    page_shell(
        "System users",
        "Disable or recover users without deleting immutable audit history.",
        view! { <section class=PANEL><div class=CLIENT_DATA_SLOT>
            {move || match users.get() {
                Some(Ok(response)) => view! { <dl class=KV_LIST><For each=move || response.users.clone() key=|user| user.user_id.clone() children=move |user| view! {
                    <dt class=KV_DT>{user.primary_email}</dt><dd class=KV_DD>{if user.disabled { "disabled" } else if user.email_verified { "active / verified" } else { "pending verification" }}</dd>
                } /></dl> }.into_any(),
                Some(Err(error)) => view! { <p class=BANNER_ERROR>{server_error_text(error)}</p> }.into_any(),
                None => view! {
                    <ListSkeleton rows=4 with_avatar=false label="Loading users" />
                }
                .into_any(),
            }}
        </div></section> },
    )
}

#[island(lazy)]
pub fn AdminHealthPage() -> impl IntoView {
    let health = browser_load(get_admin_health);
    page_shell(
        "Configuration health",
        "Verify the active storage, mail, and authorization profile.",
        view! { <section class=PANEL><div class=CLIENT_DATA_SLOT>
            {move || match health.get() {
                Some(Ok(value)) => view! { <dl class=KV_LIST>
                    <dt class=KV_DT>"Status"</dt><dd class=KV_DD>{value.status}</dd>
                    <dt class=KV_DT>"Storage"</dt><dd class=KV_DD>{value.storage_backend}</dd>
                    <dt class=KV_DT>"Mail"</dt><dd class=KV_DD>{value.mail_transport}</dd>
                    <dt class=KV_DT>"Authorization"</dt><dd class=KV_DD>{value.authorization_provider}</dd>
                </dl> }.into_any(),
                Some(Err(error)) => view! { <p class=BANNER_ERROR>{server_error_text(error)}</p> }.into_any(),
                None => view! {
                    <ListSkeleton rows=4 with_avatar=false label="Loading health" />
                }
                .into_any(),
            }}
        </div></section> },
    )
}

#[island(lazy)]
pub fn AdminPoliciesPage() -> impl IntoView {
    let versions = browser_load(list_policy_versions);
    let publish_action = ServerAction::<PublishPolicyVersion>::new();
    let (policy_text, set_policy_text) = signal(String::new());
    let (schema_text, set_schema_text) = signal(String::new());
    page_shell(
        "Cedar policy versions",
        "Validate and publish a versioned policy bundle with MFA step-up.",
        view! {
            <section class=PANEL><h2 class=PANEL_TITLE>"Published versions"</h2><div class=CLIENT_DATA_SLOT>
                {move || match versions.get() {
                    Some(Ok(response)) => view! { <dl class=KV_LIST><For each=move || response.versions.clone() key=|version| version.version_id.clone() children=move |version| view! {
                        <dt class=KV_DT>{version.version_id}</dt><dd class=KV_DD>{format!("{} / {}", version.status, version.policy_hash)}</dd>
                    } /></dl> }.into_any(),
                    Some(Err(error)) => view! { <p class=BANNER_ERROR>{server_error_text(error)}</p> }.into_any(),
                    None => view! {
                        <ListSkeleton rows=3 with_avatar=false label="Loading versions" />
                    }
                    .into_any(),
                }}
            </div></section>
            <section class=PANEL><h2 class=PANEL_TITLE>"Publish candidate"</h2>
                <label class=FIELD>
                    <span>"Cedar policy"</span>
                    <textarea
                        class=TEXTAREA
                        prop:value=move || policy_text.get()
                        on:input=move |event| set_policy_text.set(event_target_value(&event))
                    />
                </label>
                <label class=FIELD>
                    <span>"Cedar schema JSON"</span>
                    <textarea
                        class=TEXTAREA
                        prop:value=move || schema_text.get()
                        on:input=move |event| set_schema_text.set(event_target_value(&event))
                    />
                </label>
                <button type="button" class=BTN_PRIMARY on:click=move |_| {
                    publish_action.dispatch(PublishPolicyVersion {
                        policy_text: policy_text.get_untracked(),
                        schema_text: schema_text.get_untracked(),
                    });
                }>"Validate and publish"</button>
                <Show when=move || publish_action.value().get().is_some()><p class=RESULT_LINE>{move || action_result_text(publish_action.value().get())}</p></Show>
            </section>
        },
    )
}

#[island(lazy)]
pub fn ProviderConfigForm() -> impl IntoView {
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
        <section class=PANEL>
            <h2 class=PANEL_TITLE>"Provider"</h2>
            <label class=FIELD>
                <span>"Provider id"</span>
                <input
                    class=INPUT
                    type="text"
                    prop:value=move || provider_id.get()
                    on:input=move |event| set_provider_id.set(event_target_value(&event))
                />
            </label>
            <label class=INLINE_FIELD>
                <input
                    type="checkbox"
                    prop:checked=move || enabled.get()
                    on:change=move |event| set_enabled.set(event_target_checked(&event))
                />
                <span>"Enabled"</span>
            </label>
            <button type="button" class=BTN_SECONDARY disabled=move || pending.get() on:click=submit>
                "Save provider"
            </button>
            <Show when=move || value.get().is_some()>
                <p class=RESULT_LINE>{move || action_result_text(value.get())}</p>
            </Show>
        </section>
    }
}

#[island(lazy)]
pub fn SigningKeyRotationForm() -> impl IntoView {
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
        <section class=PANEL>
            <h2 class=PANEL_TITLE>"Signing key rotation"</h2>
            <p class=RESULT_LINE>"Requires a system-administrator session with MFA step-up."</p>
            <div class=CLIENT_DATA_SLOT>
                {move || match keys.get() {
                    Some(Ok(response)) if response.keys.is_empty() => view! {
                        <p class=RESULT_LINE>"No signing keys are configured."</p>
                    }.into_any(),
                    Some(Ok(response)) => view! {
                        <dl class=KV_LIST>
                            <For
                                each=move || response.keys.clone()
                                key=|key| key.kid.clone()
                                children=move |key| view! {
                                    <dt class=KV_DT>{key.kid}</dt>
                                    <dd class=KV_DD>{format!("{} / {}{}", key.alg, key.status, if key.active { " / active" } else { "" })}</dd>
                                }
                            />
                        </dl>
                    }.into_any(),
                    Some(Err(error)) => view! {
                        <p class=RESULT_LINE>{server_error_text(error)}</p>
                    }.into_any(),
                    None => view! {
                        <ListSkeleton rows=3 with_avatar=false label="Loading keys" />
                    }
                    .into_any(),
                }}
            </div>
            <label class=FIELD>
                <span>"Target key id"</span>
                <input
                    class=INPUT
                    type="text"
                    prop:value=move || kid.get()
                    on:input=move |event| set_kid.set(event_target_value(&event))
                />
            </label>
            <label class=INLINE_FIELD>
                <input
                    type="checkbox"
                    prop:checked=move || retire_previous.get()
                    on:change=move |event| set_retire_previous.set(event_target_checked(&event))
                />
                <span>"Retire previous active key"</span>
            </label>
            <button type="button" class=BTN_SECONDARY disabled=move || pending.get() on:click=submit>
                "Rotate key"
            </button>
            <Show when=move || value.get().is_some()>
                <p class=RESULT_LINE>{move || action_result_text(value.get())}</p>
            </Show>
        </section>
    }
}

#[island(lazy)]
pub fn RedirectAllowlistForm() -> impl IntoView {
    let action = ServerAction::<SaveRedirectAllowlist>::new();
    let pending = action.pending();
    let value = action.value();
    let (redirects_json, set_redirects_json) = signal("[\"/account/profile\"]".to_string());

    let submit = move |_| {
        action.dispatch(SaveRedirectAllowlist {
            redirects_json: redirects_json.get_untracked(),
        });
    };

    view! {
        <section class=PANEL>
            <h2 class=PANEL_TITLE>"Allowed redirects"</h2>
            <textarea
                class=TEXTAREA
                rows="5"
                prop:value=move || redirects_json.get()
                on:input=move |event| set_redirects_json.set(event_target_value(&event))
            />
            <button type="button" class=BTN_SECONDARY disabled=move || pending.get() on:click=submit>
                "Save allowlist"
            </button>
            <Show when=move || value.get().is_some()>
                <p class=RESULT_LINE>{move || action_result_text(value.get())}</p>
            </Show>
        </section>
    }
}
