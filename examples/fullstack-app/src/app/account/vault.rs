#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use crate::app::helpers::{redirect_browser, server_error_text, short_id_label};
use crate::app::{
    CreateDashboardSecret, DeleteDashboardSecret, ListDashboardSecrets,
    ResolveWorkspaceVaultTarget, RevealDashboardSecret, SeedDashboardDemos, browser_load,
    create_dashboard_secret, delete_dashboard_secret, list_dashboard_secrets,
    resolve_workspace_vault_target, reveal_dashboard_secret, seed_dashboard_demos,
};
use crate::contracts::{SecretCreateRequest, SecretSummary};
use crate::ui::classes::{
    ACCOUNT_PANEL, BANNER_ERROR, BANNER_OK, BTN_PRIMARY, BTN_SECONDARY, FIELD, INPUT, MONO_VALUE,
    MUTED, PANEL, RESULT_LINE, SECTION_LABEL, SR_ONLY, VAULT_ACTIONS, VAULT_ADD_INLINE,
    VAULT_COL_ACTIONS, VAULT_COL_KEY, VAULT_COL_LABEL, VAULT_COL_SCOPE, VAULT_COL_VALUE,
    VAULT_DANGER_BUTTON, VAULT_EMPTY, VAULT_EYE, VAULT_FIELD_WIDE, VAULT_FORM, VAULT_LEDE,
    VAULT_MASKED, VAULT_MODAL, VAULT_MODAL_ACTIONS, VAULT_MODAL_BACKDROP, VAULT_MODAL_BODY,
    VAULT_MODAL_CLOSE, VAULT_MODAL_CONFIRM, VAULT_MODAL_HEAD, VAULT_MODAL_HEAD_P,
    VAULT_MODAL_HEAD_TITLE, VAULT_PAGE, VAULT_PANEL_HEAD, VAULT_PANEL_HEAD_META,
    VAULT_PANEL_HEAD_TITLE, VAULT_REVEALED, VAULT_SCOPE, VAULT_TABLE, VAULT_TABLE_WRAP, VAULT_TD,
    VAULT_TD_ACTIONS, VAULT_TD_ELLIPSIS, VAULT_TD_VALUE, VAULT_TH, VAULT_TH_ACTIONS, VAULT_TRASH,
    VAULT_TRASH_ICON, VAULT_VALUE_INNER, VAULT_VALUE_INPUT_ROW, with_extra,
};
use crate::ui::{account_page_shell, page_shell};
use leptos::prelude::*;
#[cfg(feature = "hydrate")]
use leptos::task::spawn_local;
use leptos_router::hooks::use_params_map;
use server_fn::ServerFnError;
#[cfg(feature = "hydrate")]
use wasm_bindgen::prelude::*;
#[cfg(feature = "hydrate")]
use web_sys::window;

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
        <section class=PANEL>
            <p class=RESULT_LINE>"Redirecting to your workspace vault…"</p>
            <p class=MUTED>
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
            form_error.set(Some(server_error_text(e)));
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
            form_error.set(Some(server_error_text(e)));
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
            form_error.set(Some(server_error_text(e)));
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
            let msg = server_error_text(e);
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
        <div class=VAULT_PAGE>
            <section class=ACCOUNT_PANEL>
                <p class=SECTION_LABEL>"Connectors · Integrations"</p>
                <p class=VAULT_LEDE>
                    "Store API keys and passwords for REST, Postgres, and future integrations. "
                    "Resource pickers reference keys by id — plaintext never appears in list APIs."
                </p>
                <div class=VAULT_ACTIONS>
                    <a class=BTN_SECONDARY href="/dashboard">"Back to dashboard"</a>
                    <button
                        type="button"
                        class=BTN_SECONDARY
                        disabled=move || seed_action.pending().get()
                        on:click=move |_| { seed_action.dispatch(SeedDashboardDemos {}); }
                    >
                        {move || if seed_action.pending().get() { "Loading demos…" } else { "Load demo connectors" }}
                    </button>
                </div>
            </section>

            <p class=BANNER_ERROR hidden=move || form_error.get().is_none()>
                {move || form_error.get().unwrap_or_default()}
            </p>
            <p class=BANNER_OK hidden=move || form_ok.get().is_none()>
                {move || form_ok.get().unwrap_or_default()}
            </p>

            <section class=ACCOUNT_PANEL>
                <header class=VAULT_PANEL_HEAD>
                    <h2 class=VAULT_PANEL_HEAD_TITLE>"Secrets"</h2>
                    <div class=VAULT_PANEL_HEAD_META>
                        <span class=MUTED>{move || format!("{} stored", secrets.get().len())}</span>
                        <button type="button" class=with_extra(BTN_SECONDARY, Some(VAULT_ADD_INLINE)) on:click=move |_| open_create_modal()>
                            "Add secret"
                        </button>
                    </div>
                </header>
                <div class=VAULT_TABLE_WRAP>
                    <table class=VAULT_TABLE>
                        <colgroup>
                            <col class=VAULT_COL_KEY />
                            <col class=VAULT_COL_LABEL />
                            <col class=VAULT_COL_SCOPE />
                            <col class=VAULT_COL_VALUE />
                            <col class=VAULT_COL_ACTIONS />
                        </colgroup>
                        <thead>
                            <tr>
                                <th scope="col" class=VAULT_TH>"Key"</th>
                                <th scope="col" class=VAULT_TH>"Label"</th>
                                <th scope="col" class=VAULT_TH>"Scope"</th>
                                <th scope="col" class=VAULT_TH>"Value"</th>
                                <th scope="col" class=format!("{} {}", VAULT_TH, VAULT_TH_ACTIONS)><span class=SR_ONLY>"Actions"</span></th>
                            </tr>
                        </thead>
                        <tbody>
                            {move || {
                                let list = secrets.get();
                                if list.is_empty() {
                                    return view! {
                                        <tr>
                                            <td colspan="5" class=format!("{} {}", MUTED, VAULT_EMPTY)>
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
                                            <td class=format!("{} {}", MONO_VALUE, VAULT_TD_ELLIPSIS)>{key_label}</td>
                                            <td class=VAULT_TD_ELLIPSIS>{label_text}</td>
                                            <td class=VAULT_TD><span class=VAULT_SCOPE>{scope}</span></td>
                                            <td class=VAULT_TD_VALUE>
                                                <div class=VAULT_VALUE_INNER>
                                                    {move || {
                                                        let id_check = id.clone();
                                                        let revealed_map = revealed.get();
                                                        if let Some(plain) = revealed_map.get(&id_check).cloned() {
                                                            view! {
                                                                <code class=VAULT_REVEALED>{plain}</code>
                                                            }.into_any()
                                                        } else {
                                                            let id_click = id_for_reveal.clone();
                                                            let id_pend = id_for_pending.clone();
                                                            let masked_show = masked.clone();
                                                            view! {
                                                                <span class=VAULT_MASKED>{masked_show}</span>
                                                                <button
                                                                    type="button"
                                                                    class=VAULT_EYE
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
                                            <td class=VAULT_TD_ACTIONS>
                                                <button
                                                    type="button"
                                                    class=VAULT_TRASH
                                                    aria-label=format!("Delete secret {key_for_delete}")
                                                    title="Delete secret"
                                                    on:click=move |_| {
                                                        form_ok.set(None);
                                                        delete_target.set(Some((id_del.clone(), key_for_delete.clone())));
                                                    }
                                                >
                                                    <svg class=VAULT_TRASH_ICON viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">
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
                    class=VAULT_MODAL_BACKDROP
                    role="presentation"
                    on:click=move |_| create_open.set(false)
                    on:wheel=move |e| e.stop_propagation()
                >
                    <div
                        class=VAULT_MODAL
                        role="dialog"
                        aria-modal="true"
                        aria-labelledby="vault-create-title"
                        on:click=move |e| e.stop_propagation()
                    >
                        <header class=VAULT_MODAL_HEAD>
                            <div>
                                <h2 id="vault-create-title" class=VAULT_MODAL_HEAD_TITLE>"Add secret"</h2>
                                <p class=VAULT_MODAL_HEAD_P>"Keys look like environment variables. Values are encrypted with AUTH_VAULT_KEY before storage."</p>
                            </div>
                            <button type="button" class=VAULT_MODAL_CLOSE on:click=move |_| create_open.set(false)>"Close"</button>
                        </header>
                        <div class=format!("{} {}", VAULT_MODAL_BODY, "gap-4")>
                            <div class=VAULT_FORM>
                                <label class=FIELD>
                                    <span>"Key"</span>
                                    <input
                                        class=with_extra(INPUT, Some(MONO_VALUE))
                                        placeholder="STRIPE_SECRET_KEY"
                                        prop:value=move || key.get()
                                        on:input=move |e| key.set(event_target_value(&e).to_ascii_uppercase())
                                    />
                                </label>
                                <label class=FIELD>
                                    <span>"Label"</span>
                                    <input
                                        class=INPUT
                                        placeholder="Stripe live secret"
                                        prop:value=move || label.get()
                                        on:input=move |e| label.set(event_target_value(&e))
                                    />
                                </label>
                                <label class=with_extra(FIELD, Some(VAULT_FIELD_WIDE))>
                                    <span>"Description (optional)"</span>
                                    <input
                                        class=INPUT
                                        prop:value=move || description.get()
                                        on:input=move |e| description.set(event_target_value(&e))
                                    />
                                </label>
                                <label class=with_extra(FIELD, Some(VAULT_FIELD_WIDE))>
                                    <span>"Value"</span>
                                    <div class=VAULT_VALUE_INPUT_ROW>
                                        <input
                                            class=INPUT
                                            type=move || if show_value.get() { "text" } else { "password" }
                                            autocomplete="new-password"
                                            prop:value=move || value.get()
                                            on:input=move |e| value.set(event_target_value(&e))
                                        />
                                        <button
                                            type="button"
                                            class=BTN_SECONDARY
                                            on:click=move |_| show_value.update(|v| *v = !*v)
                                        >
                                            {move || if show_value.get() { "Hide" } else { "Show" }}
                                        </button>
                                    </div>
                                </label>
                            </div>
                            <div class=VAULT_MODAL_ACTIONS>
                                <button type="button" class=BTN_SECONDARY on:click=move |_| create_open.set(false)>"Cancel"</button>
                                <button
                                    type="button"
                                    class=BTN_PRIMARY
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
                    class=VAULT_MODAL_BACKDROP
                    role="presentation"
                    on:click=move |_| delete_target.set(None)
                    on:wheel=move |e| e.stop_propagation()
                >
                    <div
                        class=VAULT_MODAL_CONFIRM
                        role="dialog"
                        aria-modal="true"
                        aria-labelledby="vault-delete-title"
                        on:click=move |e| e.stop_propagation()
                    >
                        <header class=VAULT_MODAL_HEAD>
                            <div>
                                <h2 id="vault-delete-title" class=VAULT_MODAL_HEAD_TITLE>"Delete secret?"</h2>
                                <p class=VAULT_MODAL_HEAD_P>
                                    "This cannot be undone. Resources using "
                                    <strong class=MONO_VALUE>{move || delete_target.get().map(|(_, k)| k).unwrap_or_default()}</strong>
                                    " will fail until reconfigured."
                                </p>
                            </div>
                            <button type="button" class=VAULT_MODAL_CLOSE on:click=move |_| delete_target.set(None)>"Close"</button>
                        </header>
                        <div class=VAULT_MODAL_BODY>
                            <div class=VAULT_MODAL_ACTIONS>
                                <button type="button" class=BTN_SECONDARY on:click=move |_| delete_target.set(None)>"Cancel"</button>
                                <button
                                    type="button"
                                    class=with_extra(BTN_PRIMARY, Some(VAULT_DANGER_BUTTON))
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

/// Best-effort async sleep for hydrate (vault reveal auto-mask).
#[cfg(feature = "hydrate")]
async fn gloo_timers_sleep_ms(ms: u64) {
    use js_sys::Promise;
    use wasm_bindgen_futures::JsFuture;
    let promise = Promise::new(&mut |resolve, _reject| {
        if let Some(window) = web_sys::window() {
            let _ =
                window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms as i32);
        } else {
            let _ = resolve.call0(&wasm_bindgen::JsValue::NULL);
        }
    });
    let _ = JsFuture::from(promise).await;
}

#[cfg(not(feature = "hydrate"))]
async fn gloo_timers_sleep_ms(_ms: u64) {}
