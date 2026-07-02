use components::{Route, Router, Routes};
use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "hydrate")]
use wasm_bindgen::{JsCast, closure::Closure};
#[cfg(feature = "hydrate")]
use web_sys::{EventSource, MessageEvent, window};

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventLogDto {
    pub sequence: u64,
    pub event_type: String,
    pub revision: u64,
    pub payload: String,
    pub recorded_at: String,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CounterViewDto {
    pub count: i32,
    pub latest_events: Vec<EventLogDto>,
    pub last_sequence: u64,
    pub realtime_enabled: bool,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CounterRealtimeMessage {
    pub view: CounterViewDto,
    pub last_sequence: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClientCounterCommand {
    Increment(i32),
    Decrement(i32),
    Reset,
}

impl ClientCounterCommand {
    fn apply_to(self, value: i32) -> i32 {
        match self {
            Self::Increment(amount) => value.saturating_add(amount),
            Self::Decrement(amount) => value.saturating_sub(amount),
            Self::Reset => 0,
        }
    }
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
    let increment_pending = increment_action.pending();
    let decrement_pending = decrement_action.pending();
    let reset_pending = reset_action.pending();

    let (custom_amount, set_custom_amount) = signal(5);

    let counter_view = Resource::new(|| (), |_| get_counter_view());
    let (current_view, set_current_view) = signal(None::<CounterViewDto>);
    let (optimistic_count, set_optimistic_count) = signal(None::<i32>);
    let (last_seen_sequence, set_last_seen_sequence) = signal(0_u64);
    let (pending_until_sequence, set_pending_until_sequence) = signal(None::<u64>);
    let (deferred_realtime_message, set_deferred_realtime_message) =
        signal(None::<CounterRealtimeMessage>);

    // Hydrate from local cache while the first server read is in flight.
    Effect::new(move |_| {
        if optimistic_count.get().is_none() {
            #[cfg(feature = "hydrate")]
            {
                if let Some(window) = window()
                    && let Ok(Some(storage)) = window.local_storage()
                {
                    if let Ok(Some(cached_count_str)) = storage.get_item("counter_app_count")
                        && let Ok(cached_count) = cached_count_str.parse::<i32>()
                    {
                        set_optimistic_count.set(Some(cached_count));
                    }
                    if let Ok(Some(cached_sequence_str)) =
                        storage.get_item("counter_app_last_sequence")
                        && let Ok(cached_sequence) = cached_sequence_str.parse::<u64>()
                    {
                        set_last_seen_sequence.set(cached_sequence);
                    }
                }
            }
        }
    });

    Effect::new(move |_| {
        if let Some(Ok(view_data)) = counter_view.get() {
            if current_view
                .get_untracked()
                .is_some_and(|current| view_data.last_sequence < current.last_sequence)
            {
                return;
            }
            let caught_up = pending_until_sequence
                .get_untracked()
                .is_none_or(|sequence| view_data.last_sequence >= sequence);

            if caught_up {
                set_optimistic_count.set(Some(view_data.count));
                set_pending_until_sequence.set(None);
                set_deferred_realtime_message.set(None);
            }
            set_last_seen_sequence.set(view_data.last_sequence);
            set_current_view.set(Some(view_data.clone()));

            #[cfg(feature = "hydrate")]
            if caught_up
                && let Some(window) = window()
                && let Ok(Some(storage)) = window.local_storage()
            {
                let _ = storage.set_item("counter_app_count", &view_data.count.to_string());
                let _ = storage.set_item(
                    "counter_app_last_sequence",
                    &view_data.last_sequence.to_string(),
                );
            }
        }
    });

    Effect::new(move |_| {
        let mut next_view = None::<CounterViewDto>;
        let mut completed_action = false;
        for candidate in [
            increment_action.value().get(),
            decrement_action.value().get(),
            reset_action.value().get(),
        ] {
            match candidate {
                Some(Ok(view_data)) => {
                    completed_action = true;
                    if next_view
                        .as_ref()
                        .is_none_or(|current| view_data.last_sequence >= current.last_sequence)
                    {
                        next_view = Some(view_data);
                    }
                }
                Some(Err(_)) => completed_action = true,
                None => {}
            }
        }

        let local_pending =
            increment_pending.get() || decrement_pending.get() || reset_pending.get();
        let has_deferred_realtime = deferred_realtime_message.get_untracked().is_some();
        if completed_action
            && !local_pending
            && !has_deferred_realtime
            && pending_until_sequence.get_untracked().is_some()
        {
            counter_view.refetch();
        }

        if let Some(view_data) = next_view {
            if current_view
                .get_untracked()
                .is_some_and(|current| view_data.last_sequence < current.last_sequence)
            {
                return;
            }
            let caught_up = pending_until_sequence
                .get_untracked()
                .is_none_or(|sequence| view_data.last_sequence >= sequence);
            if !local_pending && !has_deferred_realtime && caught_up {
                set_optimistic_count.set(Some(view_data.count));
                set_pending_until_sequence.set(None);
            }
            set_last_seen_sequence.set(view_data.last_sequence);
            set_current_view.set(Some(view_data.clone()));

            #[cfg(feature = "hydrate")]
            if caught_up
                && let Some(window) = window()
                && let Ok(Some(storage)) = window.local_storage()
            {
                let _ = storage.set_item("counter_app_count", &view_data.count.to_string());
                let _ = storage.set_item(
                    "counter_app_last_sequence",
                    &view_data.last_sequence.to_string(),
                );
            }
        } else if completed_action
            && !local_pending
            && !has_deferred_realtime
            && let Some(view_data) = current_view.get_untracked()
            && pending_until_sequence
                .get_untracked()
                .is_none_or(|sequence| view_data.last_sequence >= sequence)
        {
            set_optimistic_count.set(Some(view_data.count));
        }
    });

    #[cfg(feature = "hydrate")]
    Effect::new(move |_| {
        let Some(Ok(view_data)) = counter_view.get() else {
            return;
        };
        if !view_data.realtime_enabled {
            return;
        }
        let url = format!(
            "/api/counter/stream?last_sequence={}",
            view_data.last_sequence
        );
        let Ok(source) = EventSource::new(&url) else {
            return;
        };
        let onmessage = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
            let Some(payload) = event.data().as_string() else {
                return;
            };
            let Ok(message) = serde_json::from_str::<CounterRealtimeMessage>(&payload) else {
                return;
            };
            if current_view
                .get_untracked()
                .is_some_and(|current| message.last_sequence < current.last_sequence)
            {
                return;
            }
            set_last_seen_sequence.set(message.last_sequence);
            set_current_view.set(Some(message.view.clone()));
            let local_pending = increment_pending.get_untracked()
                || decrement_pending.get_untracked()
                || reset_pending.get_untracked();
            let caught_up = pending_until_sequence
                .get_untracked()
                .is_none_or(|sequence| message.last_sequence >= sequence);
            if local_pending && caught_up {
                set_deferred_realtime_message.update(|deferred| {
                    if deferred
                        .as_ref()
                        .is_none_or(|current| message.last_sequence >= current.last_sequence)
                    {
                        *deferred = Some(message.clone());
                    }
                });
            } else if !local_pending && caught_up {
                set_optimistic_count.set(Some(message.view.count));
                set_deferred_realtime_message.set(None);
                set_pending_until_sequence.set(None);
            }

            if caught_up
                && let Some(window) = window()
                && let Ok(Some(storage)) = window.local_storage()
            {
                let _ = storage.set_item("counter_app_count", &message.view.count.to_string());
                let _ = storage.set_item(
                    "counter_app_last_sequence",
                    &message.last_sequence.to_string(),
                );
            }
        });
        let callback = onmessage.as_ref().unchecked_ref();
        source.set_onmessage(Some(callback));
        let _ = source.add_event_listener_with_callback("counter", callback);
        onmessage.forget();
        let cleanup_source = source.clone();
        Owner::on_cleanup(move || cleanup_source.close());
    });

    #[cfg(feature = "hydrate")]
    Effect::new(move |_| {
        let local_pending =
            increment_pending.get() || decrement_pending.get() || reset_pending.get();
        if local_pending {
            return;
        }

        let Some(message) = deferred_realtime_message.get() else {
            return;
        };
        if pending_until_sequence
            .get_untracked()
            .is_some_and(|sequence| message.last_sequence < sequence)
        {
            return;
        }

        set_last_seen_sequence.set(message.last_sequence);
        set_current_view.set(Some(message.view.clone()));
        set_optimistic_count.set(Some(message.view.count));
        set_pending_until_sequence.set(None);
        set_deferred_realtime_message.set(None);

        if let Some(window) = window()
            && let Ok(Some(storage)) = window.local_storage()
        {
            let _ = storage.set_item("counter_app_count", &message.view.count.to_string());
            let _ = storage.set_item(
                "counter_app_last_sequence",
                &message.last_sequence.to_string(),
            );
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
        pending_until_sequence.get().is_some()
            || deferred_realtime_message.get().is_some()
            || increment_pending.get()
            || decrement_pending.get()
            || reset_pending.get()
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

    let apply_optimistic_command = move |command: ClientCounterCommand| {
        let base_count = optimistic_count
            .get_untracked()
            .or_else(|| current_view.get_untracked().map(|view| view.count))
            .unwrap_or_default();
        set_optimistic_count.set(Some(command.apply_to(base_count)));
        let base_sequence = pending_until_sequence
            .get_untracked()
            .or_else(|| current_view.get_untracked().map(|view| view.last_sequence))
            .unwrap_or_else(|| last_seen_sequence.get_untracked());
        set_pending_until_sequence.set(Some(base_sequence.saturating_add(1)));
    };

    // Button click handlers. Local state updates immediately while every click
    // is sent to the server immediately; server-side retry handles stale
    // expected revisions under bursty writes.
    let on_inc = move |_| {
        let command = ClientCounterCommand::Increment(1);
        apply_optimistic_command(command);
        increment_action.dispatch(IncrementCount { amount: 1 });
    };

    let on_dec = move |_| {
        let command = ClientCounterCommand::Decrement(1);
        apply_optimistic_command(command);
        decrement_action.dispatch(DecrementCount { amount: 1 });
    };

    let on_reset = move |_| {
        let command = ClientCounterCommand::Reset;
        apply_optimistic_command(command);
        reset_action.dispatch(ResetCount {});
    };

    let on_custom_inc = move |_| {
        let val = custom_amount.get();
        let command = ClientCounterCommand::Increment(val);
        apply_optimistic_command(command);
        increment_action.dispatch(IncrementCount { amount: val });
    };

    let on_custom_dec = move |_| {
        let val = custom_amount.get();
        let command = ClientCounterCommand::Decrement(val);
        apply_optimistic_command(command);
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
                            "Built with Leptos Server Functions, WASIp2, and pluggable persistence."
                        </p>
                    </div>

                    <div class="relative bg-slate-900/60 rounded-2xl p-8 border border-slate-800/80 backdrop-blur-sm shadow-inner">
                        <div class="text-6xl md:text-7xl font-black text-white tracking-tighter tabular-nums drop-shadow-[0_2px_10px_rgba(56,189,248,0.15)]">
                            {display_count}
                        </div>
                        <div class="text-slate-500 text-xs mt-3 uppercase tracking-widest font-semibold">
                            "Live Aggregate Value"
                        </div>

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
                            class="rounded-xl bg-slate-800 hover:bg-slate-700 active:scale-95 text-slate-100 font-bold p-4 border border-slate-700/50 shadow transition-all disabled:opacity-40 disabled:cursor-not-allowed group flex flex-col items-center gap-1"
                        >
                            <span class="text-lg group-hover:scale-110 transition-transform font-black">"- 1"</span>
                            <span class="text-[10px] uppercase text-slate-400 font-medium">"Decrement"</span>
                        </button>

                        <button
                            on:click=on_reset
                            class="rounded-xl bg-amber-500/10 hover:bg-amber-500/20 active:scale-95 text-amber-400 font-bold p-4 border border-amber-500/20 shadow transition-all disabled:opacity-40 disabled:cursor-not-allowed group flex flex-col items-center gap-1"
                        >
                            <span class="text-lg group-hover:rotate-45 transition-transform font-black">"↺"</span>
                            <span class="text-[10px] uppercase text-amber-500/80 font-medium">"Reset"</span>
                        </button>

                        <button
                            on:click=on_inc
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
                                class="rounded-lg bg-slate-800/80 hover:bg-slate-800 text-slate-200 hover:text-white font-semibold text-xs py-2 px-3 border border-slate-700/50 shadow-sm transition-all disabled:opacity-40"
                            >
                                "Batch Dec (-" {custom_amount} ")"
                            </button>
                            <button
                                on:click=on_custom_inc
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
                        {move || {
                            match current_view.get() {
                                None => view! {
                                    <div class="text-center text-xs text-slate-500 py-10 font-mono">
                                        "Polling ledger stream..."
                                    </div>
                                }.into_any(),
                                Some(view_data) if view_data.latest_events.is_empty() => view! {
                                    <div class="text-center text-xs text-slate-500 py-16 font-mono border border-dashed border-slate-800 rounded-xl">
                                        "No events committed yet."
                                    </div>
                                }.into_any(),
                                Some(view_data) => {
                                    let logs = view_data.latest_events;
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
                        }}
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

#[cfg(feature = "ssr")]
pub async fn get_counter_view_db() -> Result<CounterViewDto, ServerFnError> {
    crate::application::get_counter_view()
        .await
        .map_err(server_fn_error)
}

#[cfg(feature = "ssr")]
async fn run_cqrs_command(
    command: crate::domain::CounterCommand,
) -> Result<CounterViewDto, ServerFnError> {
    crate::application::execute_counter_command(command)
        .await
        .map_err(server_fn_error)
}

#[cfg(feature = "ssr")]
fn server_fn_error(error: crate::error::CounterAppError) -> ServerFnError {
    if error.is_client_error() {
        tracing::warn!(
            error = %error,
            error_code = error.public_code(),
            "counter server function rejected request"
        );
    } else {
        tracing::error!(
            error = %error,
            error_code = error.public_code(),
            "counter server function failed"
        );
    }
    error.server_fn_error()
}

#[server(prefix = "/api")]
pub async fn get_counter_view() -> Result<CounterViewDto, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        get_counter_view_db().await
    }
    #[cfg(not(feature = "ssr"))]
    {
        unreachable!()
    }
}

#[server(prefix = "/api")]
pub async fn increment_count(amount: i32) -> Result<CounterViewDto, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        if amount <= 0 {
            return Err(server_fn_error(crate::error::CounterAppError::validation(
                "amount must be positive",
            )));
        }
        run_cqrs_command(crate::domain::CounterCommand::Increment { amount }).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = amount;
        unreachable!()
    }
}

#[server(prefix = "/api")]
pub async fn decrement_count(amount: i32) -> Result<CounterViewDto, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        if amount <= 0 {
            return Err(server_fn_error(crate::error::CounterAppError::validation(
                "amount must be positive",
            )));
        }
        run_cqrs_command(crate::domain::CounterCommand::Decrement { amount }).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = amount;
        unreachable!()
    }
}

#[server(prefix = "/api")]
pub async fn reset_count() -> Result<CounterViewDto, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        run_cqrs_command(crate::domain::CounterCommand::Reset).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        unreachable!()
    }
}
