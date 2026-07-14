//! Resources & Queries modal (Retool-style connection + query editor).

use crate::contracts::{
    ApiKeyLocation, GrpcProtoSource, HeaderBag, HeaderValue, HttpMethod, PostgresSslMode,
    QueryConfig, QueryResult, QuerySummary, QueryUpsert, ResourceAuth, ResourceConfig,
    ResourceKind, ResourceSummary, ResourceUpsert, SecretSummary, TransformStep,
};
use leptos::prelude::*;

// Parent is `app_dashboard_board`; grandparent is `app` (server fns live there).
use super::super::{
    DeleteDashboardQuery, DeleteDashboardResource, RunDashboardQuery, SeedDashboardDemos,
    UpsertDashboardQuery, UpsertDashboardResource,
};

fn event_target_value(event: &leptos::ev::Event) -> String {
    #[cfg(feature = "hydrate")]
    {
        use wasm_bindgen::JsCast;
        return event
            .target()
            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
            .map(|el| el.value())
            .or_else(|| {
                event
                    .target()
                    .and_then(|t| t.dyn_into::<web_sys::HtmlSelectElement>().ok())
                    .map(|el| el.value())
            })
            .or_else(|| {
                event
                    .target()
                    .and_then(|t| t.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
                    .map(|el| el.value())
            })
            .unwrap_or_default();
    }
    #[cfg(not(feature = "hydrate"))]
    {
        let _ = event;
        String::new()
    }
}

pub fn resources_queries_modal(
    open: ReadSignal<bool>,
    set_open: WriteSignal<bool>,
    http_enabled: bool,
    grpc_enabled: bool,
    postgres_enabled: bool,
    initial_resources: Vec<ResourceSummary>,
    initial_queries: Vec<QuerySummary>,
    initial_secrets: Vec<SecretSummary>,
) -> AnyView {
    let resources = RwSignal::new(initial_resources);
    let queries = RwSignal::new(initial_queries);
    let secrets = RwSignal::new(initial_secrets);

    let tab = RwSignal::new("catalog".to_owned()); // catalog | resource | query
    let form_error = RwSignal::new(None::<String>);
    let form_ok = RwSignal::new(None::<String>);
    let test_result = RwSignal::new(None::<QueryResult>);
    let test_tab = RwSignal::new("transformed".to_owned()); // raw | transformed | meta

    // Resource form
    let res_kind = RwSignal::new("rest".to_owned()); // rest | postgres | grpc
    let res_name = RwSignal::new(String::new());
    let res_base = RwSignal::new(String::new());
    let res_auth = RwSignal::new("none".to_owned());
    let res_secret_id = RwSignal::new(String::new());
    let res_username = RwSignal::new(String::new());
    let res_api_key_name = RwSignal::new("X-Api-Key".to_owned());
    let res_api_key_loc = RwSignal::new("header".to_owned());
    let res_oauth_token_url = RwSignal::new(String::new());
    let res_oauth_client_id = RwSignal::new(String::new());
    let res_oauth_scopes = RwSignal::new(String::new());
    let res_oauth_audience = RwSignal::new(String::new());
    let hdr_name = RwSignal::new(String::new());
    let hdr_mode = RwSignal::new("literal".to_owned());
    let hdr_literal = RwSignal::new(String::new());
    let hdr_secret_id = RwSignal::new(String::new());
    let res_headers = RwSignal::new(Vec::<HeaderBag>::new());
    // Postgres
    let pg_host = RwSignal::new("127.0.0.1".to_owned());
    let pg_port = RwSignal::new("5432".to_owned());
    let pg_database = RwSignal::new(String::new());
    let pg_user = RwSignal::new(String::new());
    let pg_ssl = RwSignal::new("prefer".to_owned());
    let pg_use_app_db = RwSignal::new(false);
    // gRPC
    let grpc_host = RwSignal::new("127.0.0.1".to_owned());
    let grpc_port = RwSignal::new("50051".to_owned());
    let grpc_tls = RwSignal::new(false);
    let grpc_gateway = RwSignal::new(String::new());

    // Query form
    let qry_name = RwSignal::new(String::new());
    let qry_resource_id = RwSignal::new(String::new());
    let qry_method = RwSignal::new("GET".to_owned());
    let qry_path = RwSignal::new("/".to_owned());
    let qry_body = RwSignal::new(String::new());
    let qry_json_path = RwSignal::new(String::new());
    let qry_as_array = RwSignal::new(true);
    let qry_limit = RwSignal::new("100".to_owned());
    let qry_sql = RwSignal::new("SELECT 1 AS value".to_owned());
    let qry_grpc_service = RwSignal::new(String::new());
    let qry_grpc_method = RwSignal::new(String::new());
    let qry_grpc_request = RwSignal::new("{}".to_owned());

    let upsert_resource = ServerAction::<UpsertDashboardResource>::new();
    let upsert_query = ServerAction::<UpsertDashboardQuery>::new();
    let delete_resource = ServerAction::<DeleteDashboardResource>::new();
    let delete_query = ServerAction::<DeleteDashboardQuery>::new();
    let run_query = ServerAction::<RunDashboardQuery>::new();
    let seed_demos = ServerAction::<SeedDashboardDemos>::new();

    Effect::new(move |_| match upsert_resource.value().get() {
        Some(Ok(summary)) => {
            resources.update(|list| {
                if let Some(slot) = list.iter_mut().find(|r| r.id == summary.id) {
                    *slot = summary.clone();
                } else {
                    list.push(summary.clone());
                }
            });
            if qry_resource_id.get_untracked().is_empty() {
                qry_resource_id.set(summary.id);
            }
            form_error.set(None);
            tab.set("query".to_owned());
        }
        Some(Err(e)) => form_error.set(Some(e.to_string())),
        None => {}
    });

    Effect::new(move |_| match upsert_query.value().get() {
        Some(Ok(summary)) => {
            queries.update(|list| {
                if let Some(slot) = list.iter_mut().find(|q| q.id == summary.id) {
                    *slot = summary.clone();
                } else {
                    list.push(summary.clone());
                }
            });
            form_error.set(None);
        }
        Some(Err(e)) => form_error.set(Some(e.to_string())),
        None => {}
    });

    Effect::new(move |_| match run_query.value().get() {
        Some(Ok(result)) => {
            test_result.set(Some(result));
            form_error.set(None);
        }
        Some(Err(e)) => form_error.set(Some(e.to_string())),
        None => {}
    });

    Effect::new(move |_| {
        if let Some(Ok(_)) = seed_demos.value().get() {
            form_ok.set(Some(
                "Demo connectors seeded. Close and refresh the board to see bound widgets.".into(),
            ));
            form_error.set(None);
        }
    });

    Effect::new(move |_| {
        if let Some(Ok(_)) = delete_resource.value().get() {
            // Best-effort: remove last targeted id is hard; reload from action isn't available.
            // Caller may hard-refresh; we filter by clearing empty names only when lists refresh via board.
        }
    });

    let secret_option_label = |s: &SecretSummary| -> String {
        let key = if s.key.is_empty() {
            s.name.clone()
        } else {
            s.key.clone()
        };
        if s.label.is_empty() || s.label == key {
            key
        } else {
            format!("{key} — {}", s.label)
        }
    };

    view! {
        {move || {
            if !open.get() {
                return view! { <></> }.into_any();
            }
            view! {
                <div
                    class="board-modal-backdrop"
                    role="presentation"
                    on:click=move |_| set_open.set(false)
                    on:wheel=move |e| e.stop_propagation()
                >
                    <div
                        class="board-modal board-modal-wide board-modal-resources"
                        role="dialog"
                        aria-modal="true"
                        on:click=move |e| e.stop_propagation()
                    >
                        <header class="board-modal-head">
                            <div>
                                <h2>"Resources & queries"</h2>
                                <p>"Connect once (auth + headers), write queries, test server-side. Secrets live in the vault — never returned after save."</p>
                            </div>
                            <button type="button" class="board-modal-close" on:click=move |_| set_open.set(false)>"Close"</button>
                        </header>

                        <div class="board-rq-tabs" role="tablist">
                            <button type="button" class="board-rq-tab" class:is-active=move || tab.get() == "catalog"
                                on:click=move |_| tab.set("catalog".into())>"Catalog"</button>
                            <button type="button" class="board-rq-tab" class:is-active=move || tab.get() == "resource"
                                on:click=move |_| tab.set("resource".into())>"Resource"</button>
                            <button type="button" class="board-rq-tab" class:is-active=move || tab.get() == "query"
                                on:click=move |_| tab.set("query".into())>"Query"</button>
                        </div>

                        <p class="error-banner" hidden=move || form_error.get().is_none()>
                            {move || form_error.get().unwrap_or_default()}
                        </p>
                        <p class="success-banner" hidden=move || form_ok.get().is_none()>
                            {move || form_ok.get().unwrap_or_default()}
                        </p>

                        <div class="board-modal-body board-rq-body">
                            <Show when=move || tab.get() == "catalog">
                                <div class="board-rq-catalog">
                                    <article class="board-rq-card" class:is-disabled=move || !http_enabled>
                                        <strong>"REST API"</strong>
                                        <p>"Base URL, Bearer / Basic / API key / OAuth2 client credentials, default headers."</p>
                                        <button type="button" class="primary-button" disabled=move || !http_enabled
                                            on:click=move |_| {
                                                res_kind.set("rest".into());
                                                tab.set("resource".into());
                                            }
                                        >"Connect REST"</button>
                                    </article>
                                    <article class="board-rq-card" class:is-disabled=move || !postgres_enabled>
                                        <strong>"PostgreSQL"</strong>
                                        <p>"Host, database, user, password secret, SSL — or App database (@app). SELECT-only SQL."</p>
                                        <button type="button" class="primary-button" disabled=move || !postgres_enabled
                                            on:click=move |_| {
                                                res_kind.set("postgres".into());
                                                tab.set("resource".into());
                                            }
                                        >"Connect Postgres"</button>
                                    </article>
                                    <article class="board-rq-card">
                                        <strong>"gRPC"</strong>
                                        <p>"Unary via JSON gateway (grpc-gateway) today. Native Spin HTTP/2 gRPC is gated."
                                            {if grpc_enabled { " (AUTH_DASHBOARD_GRPC_ENABLED=on)" } else { "" }}
                                        </p>
                                        <button type="button" class="primary-button"
                                            on:click=move |_| {
                                                res_kind.set("grpc".into());
                                                tab.set("resource".into());
                                            }
                                        >"Connect gRPC"</button>
                                    </article>
                                    <article class="board-rq-card">
                                        <strong>"App builtins"</strong>
                                        <p>"Session, orgs, audit, MFA — already on the board as widgets."</p>
                                        <button type="button" class="secondary-button" disabled=true>"Built-in"</button>
                                    </article>
                                    <article class="board-rq-card">
                                        <strong>"Demo connectors"</strong>
                                        <p>"One-click REST + app Postgres resources, queries, and bound board widgets."</p>
                                        <button
                                            type="button"
                                            class="primary-button"
                                            disabled=move || seed_demos.pending().get()
                                            on:click=move |_| { seed_demos.dispatch(SeedDashboardDemos {}); }
                                        >
                                            {move || if seed_demos.pending().get() { "Seeding…" } else { "Load demos" }}
                                        </button>
                                    </article>
                                </div>
                                <div class="board-rq-lists">
                                    <section>
                                        <h3>"Connections"</h3>
                                        <ul class="board-list">
                                            {move || {
                                                let list = resources.get();
                                                if list.is_empty() {
                                                    return view! {
                                                        <li class="board-muted">
                                                            "No resources yet. Connect REST/Postgres above, or load demos."
                                                        </li>
                                                    }.into_any();
                                                }
                                                list.into_iter().map(|res| {
                                                    let id = res.id.clone();
                                                    let id2 = res.id.clone();
                                                    view! {
                                                        <li class="board-list-row">
                                                            <div class="board-list-grow">
                                                                <strong>{res.name.clone()}</strong>
                                                                <span class="board-list-meta">{format!("{} · {} · auth={}", res.kind.label(), res.detail, res.auth_type)}</span>
                                                            </div>
                                                            <button type="button" class="secondary-button" on:click=move |_| {
                                                                qry_resource_id.set(id.clone());
                                                                tab.set("query".into());
                                                            }>"Query"</button>
                                                            <button type="button" class="board-tile-remove" aria-label="Delete resource" on:click=move |_| {
                                                                delete_resource.dispatch(DeleteDashboardResource { resource_id: id2.clone() });
                                                                resources.update(|l| l.retain(|r| r.id != id2));
                                                            }>"×"</button>
                                                        </li>
                                                    }
                                                }).collect_view().into_any()
                                            }}
                                        </ul>
                                    </section>
                                    <section>
                                        <h3>"Queries"</h3>
                                        <ul class="board-list">
                                            {move || {
                                                let list = queries.get();
                                                if list.is_empty() {
                                                    return view! { <li class="board-muted">"No queries yet."</li> }.into_any();
                                                }
                                                list.into_iter().map(|q| {
                                                    let id = q.id.clone();
                                                    let id2 = q.id.clone();
                                                    view! {
                                                        <li class="board-list-row">
                                                            <div class="board-list-grow">
                                                                <strong>{q.name.clone()}</strong>
                                                                <span class="board-list-meta">{q.detail.clone()}</span>
                                                            </div>
                                                            <button type="button" class="secondary-button" on:click=move |_| {
                                                                run_query.dispatch(RunDashboardQuery { query_id: id.clone() });
                                                                tab.set("query".into());
                                                            }>"Test"</button>
                                                            <button type="button" class="board-tile-remove" on:click=move |_| {
                                                                delete_query.dispatch(DeleteDashboardQuery { query_id: id2.clone() });
                                                                queries.update(|l| l.retain(|x| x.id != id2));
                                                            }>"×"</button>
                                                        </li>
                                                    }
                                                }).collect_view().into_any()
                                            }}
                                        </ul>
                                    </section>
                                </div>
                            </Show>

                            <Show when=move || tab.get() == "resource">
                                <section class="board-sources-form board-rq-form">
                                    <h3>{move || match res_kind.get().as_str() {
                                        "postgres" => "PostgreSQL resource",
                                        "grpc" => "gRPC resource",
                                        _ => "REST resource",
                                    }}</h3>
                                    <label class="auth-field"><span>"Kind"</span>
                                        <select class="auth-input" prop:value=move || res_kind.get()
                                            on:change=move |e| res_kind.set(event_target_value(&e))>
                                            <option value="rest">"REST API"</option>
                                            <option value="postgres">"PostgreSQL"</option>
                                            <option value="grpc">"gRPC"</option>
                                        </select>
                                    </label>
                                    <label class="auth-field"><span>"Name"</span>
                                        <input class="auth-input" prop:value=move || res_name.get()
                                            on:input=move |e| res_name.set(event_target_value(&e)) />
                                    </label>

                                    <Show when=move || res_kind.get() == "rest">
                                    <label class="auth-field"><span>"Base URL"</span>
                                        <input class="auth-input" placeholder="https://api.example.com"
                                            prop:value=move || res_base.get()
                                            on:input=move |e| res_base.set(event_target_value(&e)) />
                                    </label>
                                    <label class="auth-field"><span>"Authentication"</span>
                                        <select class="auth-input" prop:value=move || res_auth.get()
                                            on:change=move |e| res_auth.set(event_target_value(&e))>
                                            <option value="none">"None"</option>
                                            <option value="bearer">"Bearer token (secret)"</option>
                                            <option value="basic">"Basic (user + password secret)"</option>
                                            <option value="api_key">"API key"</option>
                                            <option value="oauth2_cc">"OAuth2 client credentials"</option>
                                        </select>
                                    </label>

                                    <Show when=move || matches!(res_auth.get().as_str(), "bearer" | "api_key")>
                                        <label class="auth-field"><span>"Secret"</span>
                                            <select class="auth-input" prop:value=move || res_secret_id.get()
                                                on:change=move |e| res_secret_id.set(event_target_value(&e))>
                                                <option value="">"— select vault secret —"</option>
                                                {move || secrets.get().into_iter().map(|s| {
                                                    let id = s.id.clone();
                                                    let name = secret_option_label(&s);
                                                    view! { <option value=id>{name}</option> }
                                                }).collect_view()}
                                            </select>
                                        </label>
                                        <p class="board-muted">
                                            "Empty list? Add secrets in Account → Secret vault, then reopen this dialog."
                                        </p>
                                    </Show>
                                    <Show when=move || res_auth.get() == "api_key">
                                        <div class="board-rq-row">
                                            <label class="auth-field"><span>"Key name"</span>
                                                <input class="auth-input" prop:value=move || res_api_key_name.get()
                                                    on:input=move |e| res_api_key_name.set(event_target_value(&e)) />
                                            </label>
                                            <label class="auth-field"><span>"Location"</span>
                                                <select class="auth-input" prop:value=move || res_api_key_loc.get()
                                                    on:change=move |e| res_api_key_loc.set(event_target_value(&e))>
                                                    <option value="header">"Header"</option>
                                                    <option value="query">"Query param"</option>
                                                </select>
                                            </label>
                                        </div>
                                    </Show>
                                    <Show when=move || res_auth.get() == "basic">
                                        <label class="auth-field"><span>"Username"</span>
                                            <input class="auth-input" prop:value=move || res_username.get()
                                                on:input=move |e| res_username.set(event_target_value(&e)) />
                                        </label>
                                        <label class="auth-field"><span>"Password secret"</span>
                                            <select class="auth-input" prop:value=move || res_secret_id.get()
                                                on:change=move |e| res_secret_id.set(event_target_value(&e))>
                                                <option value="">"— select vault secret —"</option>
                                                {move || secrets.get().into_iter().map(|s| {
                                                    let id = s.id.clone();
                                                    let name = secret_option_label(&s);
                                                    view! { <option value=id>{name}</option> }
                                                }).collect_view()}
                                            </select>
                                        </label>
                                    </Show>
                                    <Show when=move || res_auth.get() == "oauth2_cc">
                                        <label class="auth-field"><span>"Token URL"</span>
                                            <input class="auth-input" prop:value=move || res_oauth_token_url.get()
                                                on:input=move |e| res_oauth_token_url.set(event_target_value(&e)) />
                                        </label>
                                        <label class="auth-field"><span>"Client ID"</span>
                                            <input class="auth-input" prop:value=move || res_oauth_client_id.get()
                                                on:input=move |e| res_oauth_client_id.set(event_target_value(&e)) />
                                        </label>
                                        <label class="auth-field"><span>"Client secret"</span>
                                            <select class="auth-input" prop:value=move || res_secret_id.get()
                                                on:change=move |e| res_secret_id.set(event_target_value(&e))>
                                                <option value="">"— select vault secret —"</option>
                                                {move || secrets.get().into_iter().map(|s| {
                                                    let id = s.id.clone();
                                                    let name = secret_option_label(&s);
                                                    view! { <option value=id>{name}</option> }
                                                }).collect_view()}
                                            </select>
                                        </label>
                                        <label class="auth-field"><span>"Scopes (space-separated)"</span>
                                            <input class="auth-input" prop:value=move || res_oauth_scopes.get()
                                                on:input=move |e| res_oauth_scopes.set(event_target_value(&e)) />
                                        </label>
                                        <label class="auth-field"><span>"Audience (optional)"</span>
                                            <input class="auth-input" prop:value=move || res_oauth_audience.get()
                                                on:input=move |e| res_oauth_audience.set(event_target_value(&e)) />
                                        </label>
                                    </Show>

                                    <h3>"Default headers"</h3>
                                    <ul class="board-list">
                                        {move || res_headers.get().into_iter().enumerate().map(|(idx, h)| {
                                            let label = match &h.value {
                                                HeaderValue::Literal { value } => format!("{}: {value}", h.name),
                                                HeaderValue::Secret { secret_id } => format!("{}: secret:{secret_id}", h.name),
                                            };
                                            view! {
                                                <li class="board-list-row">
                                                    <span class="board-list-meta">{label}</span>
                                                    <button type="button" class="board-tile-remove" on:click=move |_| {
                                                        res_headers.update(|list| { list.remove(idx); });
                                                    }>"×"</button>
                                                </li>
                                            }
                                        }).collect_view()}
                                    </ul>
                                    <div class="board-rq-row">
                                        <label class="auth-field"><span>"Header name"</span>
                                            <input class="auth-input" prop:value=move || hdr_name.get()
                                                on:input=move |e| hdr_name.set(event_target_value(&e)) />
                                        </label>
                                        <label class="auth-field"><span>"Value type"</span>
                                            <select class="auth-input" prop:value=move || hdr_mode.get()
                                                on:change=move |e| hdr_mode.set(event_target_value(&e))>
                                                <option value="literal">"Literal"</option>
                                                <option value="secret">"Secret"</option>
                                            </select>
                                        </label>
                                    </div>
                                    <Show when=move || hdr_mode.get() == "literal">
                                        <label class="auth-field"><span>"Value"</span>
                                            <input class="auth-input" prop:value=move || hdr_literal.get()
                                                on:input=move |e| hdr_literal.set(event_target_value(&e)) />
                                        </label>
                                    </Show>
                                    <Show when=move || hdr_mode.get() == "secret">
                                        <label class="auth-field"><span>"Vault secret"</span>
                                            <select class="auth-input" prop:value=move || hdr_secret_id.get()
                                                on:change=move |e| hdr_secret_id.set(event_target_value(&e))>
                                                <option value="">"— select vault secret —"</option>
                                                {move || secrets.get().into_iter().map(|s| {
                                                    let id = s.id.clone();
                                                    let name = secret_option_label(&s);
                                                    view! { <option value=id>{name}</option> }
                                                }).collect_view()}
                                            </select>
                                        </label>
                                    </Show>
                                    <button type="button" class="secondary-button" on:click=move |_| {
                                        let name = hdr_name.get_untracked().trim().to_owned();
                                        if name.is_empty() { return; }
                                        let value = if hdr_mode.get_untracked() == "secret" {
                                            let sid = hdr_secret_id.get_untracked();
                                            if sid.is_empty() { return; }
                                            HeaderValue::secret(sid)
                                        } else {
                                            HeaderValue::literal(hdr_literal.get_untracked())
                                        };
                                        res_headers.update(|list| list.push(HeaderBag { name, value }));
                                        hdr_name.set(String::new());
                                        hdr_literal.set(String::new());
                                    }>"Add header"</button>
                                    </Show>

                                    <Show when=move || res_kind.get() == "postgres">
                                        <label class="board-rq-check">
                                            <input type="checkbox" prop:checked=move || pg_use_app_db.get()
                                                on:change=move |e| {
                                                    #[cfg(feature = "hydrate")]
                                                    {
                                                        use wasm_bindgen::JsCast;
                                                        if let Some(el) = e.target().and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok()) {
                                                            pg_use_app_db.set(el.checked());
                                                        }
                                                    }
                                                    #[cfg(not(feature = "hydrate"))]
                                                    { let _ = e; }
                                                }
                                            />
                                            <span>"Use app database (@app) — same Postgres as auth"</span>
                                        </label>
                                        <Show when=move || !pg_use_app_db.get()>
                                            <div class="board-rq-row">
                                                <label class="auth-field"><span>"Host"</span>
                                                    <input class="auth-input" prop:value=move || pg_host.get()
                                                        on:input=move |e| pg_host.set(event_target_value(&e)) />
                                                </label>
                                                <label class="auth-field"><span>"Port"</span>
                                                    <input class="auth-input" prop:value=move || pg_port.get()
                                                        on:input=move |e| pg_port.set(event_target_value(&e)) />
                                                </label>
                                            </div>
                                            <label class="auth-field"><span>"Database"</span>
                                                <input class="auth-input" prop:value=move || pg_database.get()
                                                    on:input=move |e| pg_database.set(event_target_value(&e)) />
                                            </label>
                                            <label class="auth-field"><span>"User"</span>
                                                <input class="auth-input" prop:value=move || pg_user.get()
                                                    on:input=move |e| pg_user.set(event_target_value(&e)) />
                                            </label>
                                            <label class="auth-field"><span>"Password secret"</span>
                                                <select class="auth-input" prop:value=move || res_secret_id.get()
                                                    on:change=move |e| res_secret_id.set(event_target_value(&e))>
                                                    <option value="">"— select vault secret —"</option>
                                                    {move || secrets.get().into_iter().map(|s| {
                                                        let id = s.id.clone();
                                                        let name = secret_option_label(&s);
                                                        view! { <option value=id>{name}</option> }
                                                    }).collect_view()}
                                                </select>
                                            </label>
                                            <label class="auth-field"><span>"SSL mode"</span>
                                                <select class="auth-input" prop:value=move || pg_ssl.get()
                                                    on:change=move |e| pg_ssl.set(event_target_value(&e))>
                                                    <option value="disable">"disable"</option>
                                                    <option value="prefer">"prefer"</option>
                                                    <option value="require">"require"</option>
                                                </select>
                                            </label>
                                        </Show>
                                        <p class="board-muted">"SQL is SELECT-only. Spin must allow outbound postgres://host:port (see spin.toml)."</p>
                                    </Show>

                                    <Show when=move || res_kind.get() == "grpc">
                                        <div class="board-rq-row">
                                            <label class="auth-field"><span>"Host"</span>
                                                <input class="auth-input" prop:value=move || grpc_host.get()
                                                    on:input=move |e| grpc_host.set(event_target_value(&e)) />
                                            </label>
                                            <label class="auth-field"><span>"Port"</span>
                                                <input class="auth-input" prop:value=move || grpc_port.get()
                                                    on:input=move |e| grpc_port.set(event_target_value(&e)) />
                                            </label>
                                        </div>
                                        <label class="board-rq-check">
                                            <input type="checkbox" prop:checked=move || grpc_tls.get()
                                                on:change=move |e| {
                                                    #[cfg(feature = "hydrate")]
                                                    {
                                                        use wasm_bindgen::JsCast;
                                                        if let Some(el) = e.target().and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok()) {
                                                            grpc_tls.set(el.checked());
                                                        }
                                                    }
                                                    #[cfg(not(feature = "hydrate"))]
                                                    { let _ = e; }
                                                }
                                            />
                                            <span>"TLS"</span>
                                        </label>
                                        <label class="auth-field"><span>"JSON gateway base URL (recommended)"</span>
                                            <input class="auth-input" placeholder="https://api.example.com"
                                                prop:value=move || grpc_gateway.get()
                                                on:input=move |e| grpc_gateway.set(event_target_value(&e)) />
                                        </label>
                                        <label class="auth-field"><span>"Auth (gateway / metadata)"</span>
                                            <select class="auth-input" prop:value=move || res_auth.get()
                                                on:change=move |e| res_auth.set(event_target_value(&e))>
                                                <option value="none">"None"</option>
                                                <option value="bearer">"Bearer secret"</option>
                                                <option value="api_key">"API key header"</option>
                                            </select>
                                        </label>
                                        <Show when=move || matches!(res_auth.get().as_str(), "bearer" | "api_key")>
                                            <label class="auth-field"><span>"Vault secret"</span>
                                                <select class="auth-input" prop:value=move || res_secret_id.get()
                                                    on:change=move |e| res_secret_id.set(event_target_value(&e))>
                                                    <option value="">"— select vault secret —"</option>
                                                    {move || secrets.get().into_iter().map(|s| {
                                                        let id = s.id.clone();
                                                        let name = secret_option_label(&s);
                                                        view! { <option value=id>{name}</option> }
                                                    }).collect_view()}
                                                </select>
                                            </label>
                                        </Show>
                                        <p class="board-muted">"Unary calls POST ProtoJSON to gateway_base_url/service/method. Without a gateway, execution returns a clear capability error."</p>
                                    </Show>

                                    <button type="button" class="primary-button" on:click=move |_| {
                                        let name = res_name.get_untracked().trim().to_owned();
                                        if name.is_empty() {
                                            form_error.set(Some("Name is required".into()));
                                            return;
                                        }
                                        let kind_str = res_kind.get_untracked();
                                        let build_auth = || -> Result<ResourceAuth, String> {
                                            Ok(match res_auth.get_untracked().as_str() {
                                                "bearer" => {
                                                    let sid = res_secret_id.get_untracked();
                                                    if sid.is_empty() { return Err("Select a bearer secret".into()); }
                                                    ResourceAuth::Bearer { secret_id: sid }
                                                }
                                                "basic" => {
                                                    let sid = res_secret_id.get_untracked();
                                                    if sid.is_empty() || res_username.get_untracked().trim().is_empty() {
                                                        return Err("Username and password secret required".into());
                                                    }
                                                    ResourceAuth::Basic {
                                                        username: res_username.get_untracked().trim().to_owned(),
                                                        password_secret_id: sid,
                                                    }
                                                }
                                                "api_key" => {
                                                    let sid = res_secret_id.get_untracked();
                                                    if sid.is_empty() { return Err("Select an API key secret".into()); }
                                                    ResourceAuth::ApiKey {
                                                        location: if res_api_key_loc.get_untracked() == "query" {
                                                            ApiKeyLocation::QueryParam
                                                        } else {
                                                            ApiKeyLocation::Header
                                                        },
                                                        name: res_api_key_name.get_untracked(),
                                                        secret_id: sid,
                                                    }
                                                }
                                                "oauth2_cc" => {
                                                    let sid = res_secret_id.get_untracked();
                                                    let token_url = res_oauth_token_url.get_untracked().trim().to_owned();
                                                    let client_id = res_oauth_client_id.get_untracked().trim().to_owned();
                                                    if sid.is_empty() || token_url.is_empty() || client_id.is_empty() {
                                                        return Err("Token URL, client id, and secret required".into());
                                                    }
                                                    let scopes = res_oauth_scopes.get_untracked().split_whitespace().map(|s| s.to_owned()).collect();
                                                    let audience = {
                                                        let a = res_oauth_audience.get_untracked().trim().to_owned();
                                                        if a.is_empty() { None } else { Some(a) }
                                                    };
                                                    ResourceAuth::OAuth2ClientCredentials {
                                                        token_url, client_id, client_secret_id: sid, scopes, audience,
                                                    }
                                                }
                                                _ => ResourceAuth::None,
                                            })
                                        };
                                        let (kind, auth, config) = match kind_str.as_str() {
                                            "postgres" => {
                                                let use_app = pg_use_app_db.get_untracked();
                                                let (host, port, database, user, password_secret_id) = if use_app {
                                                    ("@app".to_owned(), 5432u16, String::new(), String::new(), String::new())
                                                } else {
                                                    let port = pg_port.get_untracked().parse::<u16>().unwrap_or(5432);
                                                    let sid = res_secret_id.get_untracked();
                                                    if sid.is_empty() {
                                                        form_error.set(Some("Password secret required".into()));
                                                        return;
                                                    }
                                                    (
                                                        pg_host.get_untracked().trim().to_owned(),
                                                        port,
                                                        pg_database.get_untracked().trim().to_owned(),
                                                        pg_user.get_untracked().trim().to_owned(),
                                                        sid,
                                                    )
                                                };
                                                let ssl_mode = match pg_ssl.get_untracked().as_str() {
                                                    "disable" => PostgresSslMode::Disable,
                                                    "require" => PostgresSslMode::Require,
                                                    _ => PostgresSslMode::Prefer,
                                                };
                                                (ResourceKind::Postgres, ResourceAuth::None, ResourceConfig::Postgres {
                                                    host, port, database, user, password_secret_id, ssl_mode,
                                                })
                                            }
                                            "grpc" => {
                                                let auth = match build_auth() {
                                                    Ok(a) => a,
                                                    Err(e) => { form_error.set(Some(e)); return; }
                                                };
                                                let port = grpc_port.get_untracked().parse::<u16>().unwrap_or(50051);
                                                let gateway = {
                                                    let g = grpc_gateway.get_untracked().trim().to_owned();
                                                    if g.is_empty() { None } else { Some(g) }
                                                };
                                                (ResourceKind::Grpc, auth, ResourceConfig::Grpc {
                                                    host: grpc_host.get_untracked().trim().to_owned(),
                                                    port,
                                                    tls: grpc_tls.get_untracked(),
                                                    proto_source: GrpcProtoSource::Reflection,
                                                    max_message_bytes: 4 * 1024 * 1024,
                                                    use_proto_json: true,
                                                    gateway_base_url: gateway,
                                                })
                                            }
                                            _ => {
                                                let base_url = res_base.get_untracked().trim().to_owned();
                                                if base_url.is_empty() {
                                                    form_error.set(Some("Base URL is required".into()));
                                                    return;
                                                }
                                                let auth = match build_auth() {
                                                    Ok(a) => a,
                                                    Err(e) => { form_error.set(Some(e)); return; }
                                                };
                                                (ResourceKind::Rest, auth, ResourceConfig::Rest {
                                                    base_url,
                                                    timeout_ms: 15_000,
                                                })
                                            }
                                        };
                                        upsert_resource.dispatch(UpsertDashboardResource {
                                            request: ResourceUpsert {
                                                id: None,
                                                name,
                                                kind,
                                                auth,
                                                default_headers: res_headers.get_untracked(),
                                                config,
                                            },
                                        });
                                    }>"Save resource"</button>
                                </section>
                            </Show>

                            <Show when=move || tab.get() == "query">
                                <section class="board-sources-form board-rq-form">
                                    <h3>"Query against a resource"</h3>
                                    <label class="auth-field"><span>"Name"</span>
                                        <input class="auth-input" prop:value=move || qry_name.get()
                                            on:input=move |e| qry_name.set(event_target_value(&e)) />
                                    </label>
                                    <label class="auth-field"><span>"Resource"</span>
                                        <select class="auth-input" prop:value=move || qry_resource_id.get()
                                            on:change=move |e| qry_resource_id.set(event_target_value(&e))>
                                            <option value="">"— select resource —"</option>
                                            {move || resources.get().into_iter().map(|r| {
                                                view! { <option value=r.id.clone()>{format!("{} ({})", r.name, r.kind.label())}</option> }
                                            }).collect_view()}
                                        </select>
                                    </label>
                                    {
                                        // REST fields when selected resource is REST (or unknown).
                                        let is_pg = move || {
                                            let rid = qry_resource_id.get();
                                            resources.get().into_iter().any(|r| r.id == rid && matches!(r.kind, ResourceKind::Postgres))
                                        };
                                        let is_grpc = move || {
                                            let rid = qry_resource_id.get();
                                            resources.get().into_iter().any(|r| r.id == rid && matches!(r.kind, ResourceKind::Grpc))
                                        };
                                        view! {
                                            <Show when=move || !is_pg() && !is_grpc()>
                                                <div class="board-rq-row">
                                                    <label class="auth-field"><span>"Method"</span>
                                                        <select class="auth-input" prop:value=move || qry_method.get()
                                                            on:change=move |e| qry_method.set(event_target_value(&e))>
                                                            <option value="GET">"GET"</option>
                                                            <option value="POST">"POST"</option>
                                                            <option value="PUT">"PUT"</option>
                                                            <option value="PATCH">"PATCH"</option>
                                                            <option value="DELETE">"DELETE"</option>
                                                        </select>
                                                    </label>
                                                    <label class="auth-field"><span>"Path"</span>
                                                        <input class="auth-input" placeholder="/v1/items"
                                                            prop:value=move || qry_path.get()
                                                            on:input=move |e| qry_path.set(event_target_value(&e)) />
                                                    </label>
                                                </div>
                                                <label class="auth-field"><span>"Body (POST/PUT/PATCH)"</span>
                                                    <textarea class="auth-input board-rq-textarea" prop:value=move || qry_body.get()
                                                        on:input=move |e| qry_body.set(event_target_value(&e))></textarea>
                                                </label>
                                            </Show>
                                            <Show when=is_pg>
                                                <label class="auth-field"><span>"SQL (SELECT only)"</span>
                                                    <textarea class="auth-input board-rq-textarea" prop:value=move || qry_sql.get()
                                                        on:input=move |e| qry_sql.set(event_target_value(&e))></textarea>
                                                </label>
                                            </Show>
                                            <Show when=is_grpc>
                                                <label class="auth-field"><span>"Service"</span>
                                                    <input class="auth-input" placeholder="package.Service"
                                                        prop:value=move || qry_grpc_service.get()
                                                        on:input=move |e| qry_grpc_service.set(event_target_value(&e)) />
                                                </label>
                                                <label class="auth-field"><span>"Method"</span>
                                                    <input class="auth-input" placeholder="GetUser"
                                                        prop:value=move || qry_grpc_method.get()
                                                        on:input=move |e| qry_grpc_method.set(event_target_value(&e)) />
                                                </label>
                                                <label class="auth-field"><span>"Request JSON (ProtoJSON)"</span>
                                                    <textarea class="auth-input board-rq-textarea" prop:value=move || qry_grpc_request.get()
                                                        on:input=move |e| qry_grpc_request.set(event_target_value(&e))></textarea>
                                                </label>
                                            </Show>
                                        }
                                    }
                                    <h3>"Transform"</h3>
                                    <label class="auth-field"><span>"JSON path"</span>
                                        <input class="auth-input" placeholder="data.items"
                                            prop:value=move || qry_json_path.get()
                                            on:input=move |e| qry_json_path.set(event_target_value(&e)) />
                                    </label>
                                    <label class="board-rq-check">
                                        <input type="checkbox" prop:checked=move || qry_as_array.get()
                                            on:change=move |e| {
                                                #[cfg(feature = "hydrate")]
                                                {
                                                    use wasm_bindgen::JsCast;
                                                    if let Some(el) = e.target().and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok()) {
                                                        qry_as_array.set(el.checked());
                                                    }
                                                }
                                                #[cfg(not(feature = "hydrate"))]
                                                { let _ = e; }
                                            }
                                        />
                                        <span>"Normalize to array (as_array)"</span>
                                    </label>
                                    <label class="auth-field"><span>"Limit rows"</span>
                                        <input class="auth-input" prop:value=move || qry_limit.get()
                                            on:input=move |e| qry_limit.set(event_target_value(&e)) />
                                    </label>
                                    <div class="board-rq-actions">
                                        <button type="button" class="primary-button" on:click=move |_| {
                                            let name = qry_name.get_untracked().trim().to_owned();
                                            let resource_id = qry_resource_id.get_untracked();
                                            if name.is_empty() || resource_id.is_empty() {
                                                form_error.set(Some("Query name and resource are required".into()));
                                                return;
                                            }
                                            let rkind = resources
                                                .get_untracked()
                                                .into_iter()
                                                .find(|r| r.id == resource_id)
                                                .map(|r| r.kind)
                                                .unwrap_or(ResourceKind::Rest);
                                            let mut transform = Vec::new();
                                            let path = qry_json_path.get_untracked().trim().to_owned();
                                            if !path.is_empty() {
                                                transform.push(TransformStep::JsonPath { path });
                                            }
                                            if qry_as_array.get_untracked() {
                                                transform.push(TransformStep::AsArray);
                                            }
                                            if let Ok(n) = qry_limit.get_untracked().parse::<u32>() {
                                                if n > 0 {
                                                    transform.push(TransformStep::Limit { n });
                                                }
                                            }
                                            let config = match rkind {
                                                ResourceKind::Postgres => QueryConfig::Postgres {
                                                    sql: qry_sql.get_untracked(),
                                                },
                                                ResourceKind::Grpc => QueryConfig::Grpc {
                                                    service: qry_grpc_service.get_untracked().trim().to_owned(),
                                                    method: qry_grpc_method.get_untracked().trim().to_owned(),
                                                    request_json: qry_grpc_request.get_untracked(),
                                                    headers: Vec::new(),
                                                },
                                                _ => {
                                                    let method = HttpMethod::parse(&qry_method.get_untracked()).unwrap_or(HttpMethod::Get);
                                                    let body = {
                                                        let b = qry_body.get_untracked();
                                                        if b.trim().is_empty() { None } else { Some(b) }
                                                    };
                                                    QueryConfig::Rest {
                                                        method,
                                                        path: qry_path.get_untracked(),
                                                        query_params: Vec::new(),
                                                        headers: Vec::new(),
                                                        body,
                                                    }
                                                }
                                            };
                                            upsert_query.dispatch(UpsertDashboardQuery {
                                                request: QueryUpsert {
                                                    id: None,
                                                    name,
                                                    resource_id,
                                                    transform,
                                                    config,
                                                },
                                            });
                                        }>"Save query"</button>
                                        <button type="button" class="secondary-button" on:click=move |_| {
                                            // Test last saved query with matching name+resource, or first selected resource's newest query
                                            let rid = qry_resource_id.get_untracked();
                                            let name = qry_name.get_untracked().trim().to_owned();
                                            let qid = queries
                                                .get_untracked()
                                                .into_iter()
                                                .rev()
                                                .find(|q| q.resource_id == rid && (name.is_empty() || q.name == name))
                                                .map(|q| q.id)
                                                .or_else(|| {
                                                    queries.get_untracked().into_iter().rev().find(|q| q.resource_id == rid).map(|q| q.id)
                                                });
                                            if let Some(query_id) = qid {
                                                run_query.dispatch(RunDashboardQuery { query_id });
                                            } else {
                                                form_error.set(Some("Save the query first, then Test.".into()));
                                            }
                                        }>"Test"</button>
                                    </div>

                                    <Show when=move || test_result.get().is_some()>
                                        <div class="board-rq-output">
                                            <div class="board-rq-tabs" role="tablist">
                                                <button type="button" class="board-rq-tab" class:is-active=move || test_tab.get() == "raw"
                                                    on:click=move |_| test_tab.set("raw".into())>"Raw"</button>
                                                <button type="button" class="board-rq-tab" class:is-active=move || test_tab.get() == "transformed"
                                                    on:click=move |_| test_tab.set("transformed".into())>"Transformed"</button>
                                                <button type="button" class="board-rq-tab" class:is-active=move || test_tab.get() == "meta"
                                                    on:click=move |_| test_tab.set("meta".into())>"Meta"</button>
                                            </div>
                                            <pre class="board-json-preview board-json-preview-lg">{move || {
                                                match test_result.get() {
                                                    None => String::new(),
                                                    Some(r) => match test_tab.get().as_str() {
                                                        "raw" => r.raw_json,
                                                        "meta" => format!(
                                                            "ok={}\nerror={:?}\nkind={:?}\nstatus={:?}\nduration_ms={}\nrow_count={:?}\ntruncated={}",
                                                            r.ok, r.error, r.meta.resource_kind, r.meta.status,
                                                            r.meta.duration_ms, r.meta.row_count, r.meta.truncated
                                                        ),
                                                        _ => r.data_json,
                                                    },
                                                }
                                            }}</pre>
                                        </div>
                                    </Show>
                                </section>
                            </Show>
                        </div>
                    </div>
                </div>
            }.into_any()
        }}
    }
    .into_any()
}
