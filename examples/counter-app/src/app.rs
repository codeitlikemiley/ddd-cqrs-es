use components::{Route, Router, Routes};
use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "hydrate")]
use web_sys::window;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventLogDto {
    pub sequence: u64,
    pub event_type: String,
    pub revision: u64,
    pub payload: String,
    pub recorded_at: String,
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

    let fallback = || view! { "Page not found." }.into_view();

    view! {
        <Stylesheet id="leptos" href="/pkg/counter_app.css" />
        <Meta
            name="description"
            content="An Event Sourced Counter application running as a WASIp3 Component :D"
        />

        <Title text="CQRS Event Sourced Counter" />

        <Router>
            <main>
                <Routes fallback>
                    <Route path=path!("") view=HomePage />
                    <Route path=path!("/*any") view=NotFound />
                </Routes>
            </main>
        </Router>
    }
}

#[component]
fn HomePage() -> impl IntoView {
    let increment_action = ServerAction::<IncrementCount>::new();
    let decrement_action = ServerAction::<DecrementCount>::new();
    let reset_action = ServerAction::<ResetCount>::new();

    let (custom_amount, set_custom_amount) = signal(5);

    // Reactive count resource
    let count = Resource::new(
        move || {
            (
                increment_action.version().get(),
                decrement_action.version().get(),
                reset_action.version().get(),
            )
        },
        |_| get_count()
    );

    // Reactive events ledger resource
    let events_resource = Resource::new(
        move || {
            (
                increment_action.version().get(),
                decrement_action.version().get(),
                reset_action.version().get(),
            )
        },
        |_| get_latest_events()
    );

    let (optimistic_count, set_optimistic_count) = signal(None::<i32>);

    // Handle LocalStorage cache or syncing of resource
    Effect::new(move |_| {
        if optimistic_count.get().is_none() {
            #[cfg(feature = "hydrate")]
            {
                if let Some(window) = window() {
                    if let Ok(Some(storage)) = window.local_storage() {
                        if let Ok(Some(cached_count_str)) = storage.get_item("counter_app_count") {
                            if let Ok(cached_count) = cached_count_str.parse::<i32>() {
                                set_optimistic_count.set(Some(cached_count));
                                return;
                            }
                        }
                    }
                }
            }

            if let Some(Ok(server_count)) = count.get() {
                set_optimistic_count.set(Some(server_count));

                #[cfg(feature = "hydrate")]
                {
                    if let Some(window) = window() {
                        if let Ok(Some(storage)) = window.local_storage() {
                            let _ = storage.set_item("counter_app_count", &server_count.to_string());
                        }
                    }
                }
            }
        }
    });

    Effect::new(move |_| {
        if let Some(Ok(server_count)) = count.get() {
            if let Some(current_optimistic) = optimistic_count.get() {
                if server_count != current_optimistic {
                    set_optimistic_count.set(Some(server_count));
                }
            }

            #[cfg(feature = "hydrate")]
            {
                if let Some(window) = window() {
                    if let Ok(Some(storage)) = window.local_storage() {
                        let _ = storage.set_item("counter_app_count", &server_count.to_string());
                    }
                }
            }
        }
    });

    let display_count = move || {
        if let Some(opt_count) = optimistic_count.get() {
            opt_count.to_string()
        } else {
            "...".to_string()
        }
    };

    let is_pending = move || {
        increment_action.pending().get()
            || decrement_action.pending().get()
            || reset_action.pending().get()
    };

    let latest_error = move || {
        if let Some(Err(e)) = increment_action.value().get() {
            Some(e.to_string())
        } else if let Some(Err(e)) = decrement_action.value().get() {
            Some(e.to_string())
        } else if let Some(Err(e)) = reset_action.value().get() {
            Some(e.to_string())
        } else {
            None
        }
    };

    let status = move || {
        if is_pending() {
            "Syncing"
        } else if latest_error().is_some() {
            "Error"
        } else {
            "Ready"
        }
    };

    // Button click handlers
    let on_inc = move |_| {
        increment_action.dispatch(IncrementCount { amount: 1 });
    };

    let on_dec = move |_| {
        decrement_action.dispatch(DecrementCount { amount: 1 });
    };

    let on_reset = move |_| {
        reset_action.dispatch(ResetCount {});
    };

    let on_custom_inc = move |_| {
        let val = custom_amount.get();
        increment_action.dispatch(IncrementCount { amount: val });
    };

    let on_custom_dec = move |_| {
        let val = custom_amount.get();
        decrement_action.dispatch(DecrementCount { amount: val });
    };

    view! {
        <div class="min-h-screen bg-[#0f172a] text-slate-100 flex flex-col md:flex-row items-center justify-center p-6 md:p-12 gap-8 font-sans">
            <div class="bg-[#1e293b] rounded-2xl shadow-2xl p-8 md:p-10 max-w-lg w-full border border-slate-700/60 relative overflow-hidden transition-all duration-300 hover:shadow-[#38bdf8]/10 hover:shadow-2xl">
                <div class="absolute top-0 right-0 w-32 h-32 bg-sky-500/10 rounded-full blur-3xl -mr-16 -mt-16"></div>
                
                <div class="text-center space-y-6 relative z-10">
                    <div class="space-y-2">
                        <div class="flex items-center justify-center gap-3">
                            <div class="w-12 h-12 bg-gradient-to-tr from-sky-400 to-blue-500 rounded-xl flex items-center justify-center shadow-lg shadow-sky-500/20">
                                <span class="text-white font-extrabold text-2xl">C</span>
                            </div>
                            <div class="text-left">
                                <h1 class="text-2xl md:text-3xl font-extrabold tracking-tight bg-clip-text text-transparent bg-gradient-to-r from-white via-slate-100 to-sky-300">
                                    "CQRS Counter"
                                </h1>
                                <p class="text-xs text-sky-400 font-mono tracking-widest uppercase">
                                    "Platform & Integration Demo"
                                </p>
                            </div>
                        </div>
                        <p class="text-slate-400 text-xs">
                            "Built with Leptos Server Functions, WASIp2, and Spin Host SQLite."
                        </p>
                    </div>

                    <div class="relative bg-slate-900/60 rounded-2xl p-8 border border-slate-800/80 backdrop-blur-sm shadow-inner">
                        <div class="text-6xl md:text-7xl font-black text-white tracking-tighter tabular-nums drop-shadow-[0_2px_10px_rgba(56,189,248,0.15)]">
                            {display_count}
                        </div>
                        <div class="text-slate-500 text-xs mt-3 uppercase tracking-widest font-semibold">
                            "Live Aggregate Value"
                        </div>

                        <Show when=is_pending>
                            <div class="absolute inset-0 flex items-center justify-center bg-slate-900/40 rounded-2xl backdrop-blur-[1px] transition-all">
                                <div class="relative w-12 h-12">
                                    <div class="absolute inset-0 rounded-full border-4 border-slate-800"></div>
                                    <div class="absolute inset-0 rounded-full border-4 border-sky-400 border-t-transparent animate-spin"></div>
                                </div>
                            </div>
                        </Show>
                    </div>

                    <Show when=move || latest_error().is_some()>
                        <div class="bg-red-500/15 border border-red-500/30 rounded-xl p-4 text-left space-y-1">
                            <div class="flex items-center gap-2 text-red-400 font-bold text-sm">
                                <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
                                    <path fill-rule="evenodd" d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-7 4a1 1 0 11-2 0 1 1 0 012 0zm-1-9a1 1 0 00-1 1v4a1 1 0 102 0V6a1 1 0 00-1-1z" clip-rule="evenodd" />
                                </svg>
                                "Constraint Validation Error"
                            </div>
                            <p class="text-xs text-red-300 font-mono">
                                {latest_error}
                            </p>
                        </div>
                    </Show>

                    <div class="grid grid-cols-3 gap-3">
                        <button
                            on:click=on_dec
                            disabled=is_pending
                            class="rounded-xl bg-slate-800 hover:bg-slate-700 active:scale-95 text-slate-100 font-bold p-4 border border-slate-700/50 shadow transition-all disabled:opacity-40 disabled:cursor-not-allowed group flex flex-col items-center gap-1"
                        >
                            <span class="text-lg group-hover:scale-110 transition-transform font-black">"- 1"</span>
                            <span class="text-[10px] uppercase text-slate-400 font-medium">"Decrement"</span>
                        </button>
                        
                        <button
                            on:click=on_reset
                            disabled=is_pending
                            class="rounded-xl bg-amber-500/10 hover:bg-amber-500/20 active:scale-95 text-amber-400 font-bold p-4 border border-amber-500/20 shadow transition-all disabled:opacity-40 disabled:cursor-not-allowed group flex flex-col items-center gap-1"
                        >
                            <span class="text-lg group-hover:rotate-45 transition-transform font-black">"↺"</span>
                            <span class="text-[10px] uppercase text-amber-500/80 font-medium">"Reset"</span>
                        </button>

                        <button
                            on:click=on_inc
                            disabled=is_pending
                            class="rounded-xl bg-sky-500/10 hover:bg-sky-500/20 active:scale-95 text-sky-400 font-bold p-4 border border-sky-500/20 shadow transition-all disabled:opacity-40 disabled:cursor-not-allowed group flex flex-col items-center gap-1"
                        >
                            <span class="text-lg group-hover:scale-110 transition-transform font-black font-extrabold animate-pulse">"+ 1"</span>
                            <span class="text-[10px] uppercase text-sky-400 font-medium">"Increment"</span>
                        </button>
                    </div>

                    <div class="bg-slate-900/40 rounded-xl p-5 border border-slate-800/80 space-y-4">
                        <div class="flex justify-between items-center">
                            <span class="text-xs font-bold text-slate-400 uppercase tracking-widest">"Batch Operations"</span>
                            <span class="text-xs font-mono text-sky-400 bg-sky-950 px-2 py-0.5 rounded-full border border-sky-900/50">
                                "Amount: " {custom_amount}
                            </span>
                        </div>
                        <div class="flex gap-4 items-center">
                            <input
                                type="range"
                                min="1"
                                max="100"
                                prop:value=custom_amount
                                on:input=move |ev| {
                                    if let Ok(v) = event_target_value(&ev).parse::<i32>() {
                                        set_custom_amount.set(v);
                                    }
                                }
                                class="w-full h-1.5 bg-slate-800 rounded-lg appearance-none cursor-pointer accent-sky-400"
                            />
                            <input
                                type="number"
                                min="1"
                                prop:value=custom_amount
                                on:input=move |ev| {
                                    if let Ok(v) = event_target_value(&ev).parse::<i32>() {
                                        set_custom_amount.set(v);
                                    }
                                }
                                class="w-16 bg-slate-950 border border-slate-800 rounded-lg py-1 px-2 text-center text-sm text-sky-400 font-mono font-bold focus:outline-none focus:border-sky-500"
                            />
                        </div>
                        <div class="grid grid-cols-2 gap-3">
                            <button
                                on:click=on_custom_dec
                                disabled=is_pending
                                class="rounded-lg bg-slate-800/80 hover:bg-slate-800 text-slate-200 hover:text-white font-semibold text-xs py-2 px-3 border border-slate-700/50 shadow-sm transition-all disabled:opacity-40"
                            >
                                "Batch Dec (-" {custom_amount} ")"
                            </button>
                            <button
                                on:click=on_custom_inc
                                disabled=is_pending
                                class="rounded-lg bg-sky-500/10 hover:bg-sky-500/20 text-sky-400 font-semibold text-xs py-2 px-3 border border-sky-500/20 shadow-sm transition-all disabled:opacity-40"
                            >
                                "Batch Inc (+" {custom_amount} ")"
                            </button>
                        </div>
                    </div>

                    <div class="flex items-center justify-center gap-2 text-xs font-mono">
                        <div class=move || {
                            match status() {
                                "Syncing" => "w-2.5 h-2.5 rounded-full bg-sky-400 animate-pulse",
                                "Error" => "w-2.5 h-2.5 rounded-full bg-red-500",
                                _ => "w-2.5 h-2.5 rounded-full bg-emerald-500 shadow-[0_0_8px_rgba(16,185,129,0.5)]",
                            }
                        }></div>
                        <span class="text-slate-400 uppercase tracking-widest text-[10px]">
                            "System Status: "
                        </span>
                        <span class=move || {
                            match status() {
                                "Syncing" => "text-sky-400 font-bold uppercase",
                                "Error" => "text-red-400 font-bold uppercase",
                                _ => "text-emerald-400 font-bold uppercase",
                            }
                        }>
                            {status}
                        </span>
                    </div>
                </div>
            </div>

            <div class="bg-[#111827] rounded-2xl shadow-2xl p-8 max-w-md w-full border border-slate-800/80 space-y-6 flex flex-col justify-between h-[520px]">
                <div class="space-y-4 overflow-hidden flex flex-col h-full">
                    <div class="flex items-center justify-between">
                        <h2 class="text-md font-extrabold text-slate-100 tracking-wide uppercase flex items-center gap-2">
                            <span class="w-2 h-2 rounded-full bg-sky-400 animate-pulse"></span>
                            "Event Sourcing Ledger"
                        </h2>
                        <span class="text-[10px] font-mono text-slate-500 bg-slate-900 px-2 py-0.5 rounded-md border border-slate-800">
                            "Last 5 Committed"
                        </span>
                    </div>
                    <p class="text-slate-400 text-xs">
                        "Every action appends an immutable event. The read model is then updated via the CQRS projection runner."
                    </p>

                    <div class="space-y-3 overflow-y-auto pr-1 flex-1">
                        <Suspense fallback=move || view! {
                            <div class="text-center text-xs text-slate-500 py-10 font-mono">
                                "Polling ledger stream..."
                            </div>
                        }>
                            {move || {
                                events_resource.get().map(|res| {
                                    match res {
                                        Ok(logs) => {
                                            if logs.is_empty() {
                                                view! {
                                                    <div class="text-center text-xs text-slate-500 py-16 font-mono border border-dashed border-slate-800 rounded-xl">
                                                        "No events committed yet."
                                                    </div>
                                                }.into_any()
                                            } else {
                                                view! {
                                                    <div class="space-y-3">
                                                        {logs.into_iter().map(|log| {
                                                            let event_style = match log.event_type.as_str() {
                                                                "incremented" => "bg-sky-500/10 border-sky-500/20 text-sky-400",
                                                                "decremented" => "bg-slate-800 border-slate-700/60 text-slate-300",
                                                                _ => "bg-amber-500/10 border-amber-500/20 text-amber-400",
                                                            };
                                                            view! {
                                                                <div class="p-3 bg-slate-900/60 rounded-xl border border-slate-800/80 flex flex-col gap-1.5 transition-all hover:bg-slate-900 font-mono text-[11px]">
                                                                    <div class="flex justify-between items-center">
                                                                        <span class=format!("px-2 py-0.5 rounded-full font-bold text-[9px] uppercase border {}", event_style)>
                                                                            {log.event_type}
                                                                        </span>
                                                                        <span class="text-slate-500 text-[10px] font-bold">
                                                                            "#" {log.sequence}
                                                                        </span>
                                                                    </div>
                                                                    <div class="flex justify-between text-slate-400">
                                                                        <span>"Revision: " <strong class="text-slate-300">{log.revision}</strong></span>
                                                                        <span class="text-slate-500">{log.recorded_at}</span>
                                                                    </div>
                                                                    <div class="text-slate-400 truncate bg-slate-950/40 p-1.5 rounded border border-slate-900/60 text-[10px] text-left">
                                                                        "Payload: " <code class="text-slate-300">{log.payload}</code>
                                                                    </div>
                                                                </div>
                                                            }
                                                        }).collect_view()}
                                                    </div>
                                                }.into_any()
                                            }
                                        }
                                        Err(e) => {
                                            view! {
                                                <div class="text-center text-xs text-red-400 py-16 font-mono border border-red-950/20 bg-red-950/5 rounded-xl">
                                                    "Failed to load ledger: " {e.to_string()}
                                                </div>
                                            }.into_any()
                                        }
                                    }
                                })
                            }}
                        </Suspense>
                    </div>
                </div>

                <div class="pt-4 border-t border-slate-800/80 flex justify-between items-center text-[10px] font-mono text-slate-500">
                    <span>"Engine: WASM Component"</span>
                    <span>"Target: wasm32-wasip2"</span>
                </div>
            </div>
        </div>
    }
}

#[component]
fn NotFound() -> impl IntoView {
    #[cfg(feature = "ssr")]
    {
        if let Some(resp) = use_context::<leptos_wasi::response::ResponseOptions>() {
            resp.set_status(leptos_wasi::prelude::StatusCode::NOT_FOUND);
        }
    }

    view! { <h1>"Not Found"</h1> }
}

#[cfg(all(feature = "ssr", runtime_spin))]
pub async fn get_count_db() -> Result<i32, ServerFnError> {
    use spin_sdk::sqlite::{Connection, Value};
    use crate::store::SpinSqliteEventStore;
    use crate::domain::Counter;

    let event_store = SpinSqliteEventStore::<Counter>::new("default");
    let _ = event_store.initialize_schema();

    let connection = Connection::open("default").await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let query = "SELECT value FROM counter_read_model WHERE id = ?";
    let aggregate_id = crate::domain::CounterId("global".to_string());
    let aggregate_id_str = serde_json::to_string(&aggregate_id)
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    
    let params = vec![Value::Text(aggregate_id_str)];
    let rowset = connection.execute(query, params).await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let rows = rowset.collect().await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    if let Some(row) = rows.first() {
        if let Some(val) = row.get::<i64>(0) {
            return Ok(val as i32);
        }
    }

    Ok(0)
}

#[cfg(all(feature = "ssr", runtime_spin))]
pub async fn get_latest_events_db() -> Result<Vec<EventLogDto>, ServerFnError> {
    use spin_sdk::sqlite::{Connection, Value};
    use crate::store::SpinSqliteEventStore;
    use crate::domain::Counter;

    let event_store = SpinSqliteEventStore::<Counter>::new("default");
    let _ = event_store.initialize_schema();

    let connection = Connection::open("default").await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    // Query the latest 5 events
    let query = "SELECT sequence, event_type, revision, payload, recorded_at_ms FROM events ORDER BY sequence DESC LIMIT 5";
    let rowset = connection.execute(query, Vec::<Value>::new()).await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let rows = rowset.collect().await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let mut events = Vec::new();
    for row in rows {
        let sequence = row.get::<i64>(0).unwrap_or(0) as u64;
        let event_type = row.get::<&str>(1).unwrap_or("").to_string();
        let revision = row.get::<i64>(2).unwrap_or(0) as u64;
        let payload = row.get::<&str>(3).unwrap_or("").to_string();
        let recorded_at_ms = row.get::<i64>(4).unwrap_or(0);

        let recorded_at = format!("+{}ms", recorded_at_ms % 100000);

        events.push(EventLogDto {
            sequence,
            event_type,
            revision,
            payload,
            recorded_at,
        });
    }

    Ok(events)
}

#[cfg(all(feature = "ssr", runtime_wasmtime))]
pub async fn get_count_db() -> Result<i32, ServerFnError> {
    use std::fs;
    use std::path::Path;
    use crate::store::SpinSqliteEventStore;
    use crate::domain::Counter;

    let event_store = SpinSqliteEventStore::<Counter>::new("default");
    let _ = event_store.initialize_schema();

    let path = Path::new("/data/counter_read_model.json");
    if !path.exists() {
        return Ok(0);
    }

    let content = fs::read_to_string(path)
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    
    let map: std::collections::HashMap<String, i32> = serde_json::from_str(&content)
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    
    let aggregate_id = crate::domain::CounterId("global".to_string());
    let aggregate_id_str = serde_json::to_string(&aggregate_id)
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    Ok(map.get(&aggregate_id_str).copied().unwrap_or(0))
}

#[cfg(all(feature = "ssr", runtime_wasmtime))]
pub async fn get_latest_events_db() -> Result<Vec<EventLogDto>, ServerFnError> {
    use std::fs;
    use std::path::Path;
    use ddd_cqrs_es::Aggregate;
    use crate::store::SpinSqliteEventStore;
    use crate::domain::Counter;

    let event_store = SpinSqliteEventStore::<Counter>::new("default");
    let _ = event_store.initialize_schema();

    let path = Path::new("/data/events.json");
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(path)
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    
    let values: Vec<serde_json::Value> = serde_json::from_str(&content)
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let mut events = Vec::new();
    let mut matching_vals: Vec<serde_json::Value> = values.into_iter()
        .filter(|val| {
            val.get("aggregate_type").and_then(|t| t.as_str()) == Some(Counter::aggregate_type())
        })
        .collect();
    
    matching_vals.sort_by_key(|val| val.get("sequence").and_then(|s| s.as_u64()).unwrap_or(0));
    matching_vals.reverse();

    for val in matching_vals.into_iter().take(5) {
        let sequence = val.get("sequence").and_then(|s| s.as_u64()).unwrap_or(0);
        let event_type = val.get("event_type").and_then(|t| t.as_str()).unwrap_or("").to_string();
        let revision = val.get("revision").and_then(|r| r.as_u64()).unwrap_or(0);
        let payload = val.get("payload").map(|p| p.to_string()).unwrap_or_default();
        let recorded_at_ms = val.get("recorded_at_ms").and_then(|r| r.as_i64()).unwrap_or(0);

        let recorded_at = format!("+{}ms", recorded_at_ms % 100000);

        events.push(EventLogDto {
            sequence,
            event_type,
            revision,
            payload,
            recorded_at,
        });
    }

    Ok(events)
}

#[cfg(feature = "ssr")]
fn run_cqrs_command(command: crate::domain::CounterCommand) -> Result<(), ServerFnError> {
    use ddd_cqrs_es::{Repository, PersistedProjectionRunner};
    use crate::store::{SpinSqliteEventStore, SpinSqliteCheckpointStore, CounterProjection};
    use crate::domain::{Counter, CounterId};

    let event_store = SpinSqliteEventStore::<Counter>::new("default");
    
    // Ensure table schemas are initialized
    event_store.initialize_schema().map_err(|e| ServerFnError::new(e))?;

    let repo = Repository::new(event_store.clone());
    let aggregate_id = CounterId("global".to_string());

    // Execute the command through the repository
    repo.execute(&aggregate_id, command, ddd_cqrs_es::Metadata::default())
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    // Advance projection
    let checkpoint_store = SpinSqliteCheckpointStore::new("default");
    let projection = CounterProjection::new("default");
    let mut runner = PersistedProjectionRunner::new(projection, checkpoint_store);

    runner.run::<Counter, _>(&event_store)
        .map_err(|e| ServerFnError::new(format!("{:?}", e)))?;

    Ok(())
}

#[server(prefix = "/api")]
pub async fn get_count() -> Result<i32, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        get_count_db().await
    }
    #[cfg(not(feature = "ssr"))]
    {
        unreachable!()
    }
}

#[server(prefix = "/api")]
pub async fn increment_count(amount: i32) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        if amount <= 0 {
            return Err(ServerFnError::new("Amount must be positive"));
        }
        run_cqrs_command(crate::domain::CounterCommand::Increment { amount })
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = amount;
        unreachable!()
    }
}

#[server(prefix = "/api")]
pub async fn decrement_count(amount: i32) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        if amount <= 0 {
            return Err(ServerFnError::new("Amount must be positive"));
        }
        run_cqrs_command(crate::domain::CounterCommand::Decrement { amount })
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = amount;
        unreachable!()
    }
}

#[server(prefix = "/api")]
pub async fn reset_count() -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        run_cqrs_command(crate::domain::CounterCommand::Reset)
    }
    #[cfg(not(feature = "ssr"))]
    {
        unreachable!()
    }
}

#[server(prefix = "/api")]
pub async fn get_latest_events() -> Result<Vec<EventLogDto>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        get_latest_events_db().await
    }
    #[cfg(not(feature = "ssr"))]
    {
        unreachable!()
    }
}
