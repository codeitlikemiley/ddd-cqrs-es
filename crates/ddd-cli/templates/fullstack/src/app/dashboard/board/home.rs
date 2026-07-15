//! Dashboard page island (board chrome + layout state).

#![allow(unused_imports)]

use super::layout::{
    collect_placed_kinds, commit_layout, next_node_id, remove_node, reorder_siblings,
};
use super::render::render_node_list;
use crate::app::dashboard::resources as dashboard_resources;
use crate::app::helpers::server_error_text;
use crate::app::{
    DismissDashboardNotification, SaveDashboardLayout, UpdateDashboardNote, browser_load,
    get_dashboard_snapshot,
};
use crate::contracts::{
    BoardContainerKind, BoardNode, DashboardLayout, DashboardNotification, DashboardSnapshot,
    DashboardWidgetKind, HttpDisplayMode, HttpQueryResult, QueryResult, QuerySummary, WidgetBind,
};
use leptos::prelude::*;
#[cfg(feature = "hydrate")]
use leptos::task::spawn_local;
#[cfg(feature = "hydrate")]
use wasm_bindgen::JsCast;

#[component]
pub fn DashboardPage() -> impl IntoView {
    view! { <DashboardHome /> }
}

#[island]
pub fn DashboardHome() -> impl IntoView {
    let board = browser_load(get_dashboard_snapshot);
    let save_layout = ServerAction::<SaveDashboardLayout>::new();
    let dismiss_action = ServerAction::<DismissDashboardNotification>::new();
    let note_action = ServerAction::<UpdateDashboardNote>::new();

    let layout = RwSignal::new(DashboardLayout {
        version: 2,
        nodes: Vec::new(),
        widgets: Vec::new(),
    });
    let (notifications, set_notifications) = signal(Vec::<DashboardNotification>::new());
    let (snapshot, set_snapshot) = signal(None::<DashboardSnapshot>);
    let (editing, set_editing) = signal(false);
    let (picker_open, set_picker_open) = signal(false);
    let (sources_open, set_sources_open) = signal(false);
    let drag_id = RwSignal::new(None::<String>);
    let drop_target = RwSignal::new(None::<String>);
    let (seeded, set_seeded) = signal(false);
    let (save_error, set_save_error) = signal(None::<String>);

    Effect::new(move |_| {
        if let Some(Ok(data)) = board.get() {
            if !seeded.get_untracked() {
                let mut lay = data.layout.clone();
                lay.migrate_if_needed();
                layout.set(lay);
                set_notifications.set(data.notifications.clone());
                set_seeded.set(true);
            }
            set_snapshot.set(Some(data));
        }
    });

    Effect::new(move |_| match save_layout.value().get() {
        Some(Ok(next)) => {
            layout.set(next);
            set_save_error.set(None);
        }
        Some(Err(error)) => set_save_error.set(Some(server_error_text(error))),
        None => {}
    });

    Effect::new(move |_| {
        if let Some(Ok(list)) = dismiss_action.value().get() {
            set_notifications.set(list);
        }
    });

    Effect::new(move |_| {
        if let Some(Ok(next)) = note_action.value().get() {
            layout.set(next);
        }
    });

    // Lock document scroll while any board modal is open (prevents background scroll-through).
    Effect::new(move |_| {
        let open = picker_open.get() || sources_open.get();
        #[cfg(feature = "hydrate")]
        {
            if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                if let Some(root) = document.document_element() {
                    let _ = if open {
                        root.class_list().add_1("board-modal-open")
                    } else {
                        root.class_list().remove_1("board-modal-open")
                    };
                }
            }
        }
        #[cfg(not(feature = "hydrate"))]
        {
            let _ = open;
        }
    });

    view! {
        <div class="board-page" class:is-editing=move || editing.get()>
            {move || match board.get() {
                None => view! {
                    <div class="board-skeleton" aria-busy="true">
                        <div class="board-skeleton-bar"></div>
                        <div class="board-skeleton-grid">
                            <span></span><span></span><span></span><span></span>
                            <span class="span-2"></span><span class="span-2"></span>
                        </div>
                    </div>
                }.into_any(),
                Some(Err(error)) => view! {
                    <section class="board-empty">
                        <p class="error-banner">{server_error_text(error)}</p>
                    </section>
                }.into_any(),
                Some(Ok(_)) => {
                    let data = snapshot.get().or_else(|| board.get().and_then(Result::ok));
                    let Some(data) = data else {
                        return view! { <div class="board-skeleton" aria-busy="true"></div> }.into_any();
                    };
                    let greeting = data.greeting_name.clone();
                    let has_tenant = data.has_tenant;
                    let tenant_label = data.tenant_label.clone();
                    let org_count = data.organization_count;
                    let http_enabled = data.http_enabled;
                    let first_http_source_id =
                        data.data_sources.first().map(|s| s.id.clone())
                            .or_else(|| data.queries.first().map(|q| q.id.clone()));
                    let http_results = data.http_results.clone();
                    let query_results = data.query_results.clone();
                    let query_summaries_for_board = data.queries.clone();
                    let resource_summaries = data.resources.clone();
                    let query_summaries = data.queries.clone();
                    let secrets_for_modal = data.secrets.clone();
                    let http_enabled_flag = http_enabled;
                    let grpc_enabled = data.grpc_resources_enabled;
                    let postgres_enabled = data.postgres_resources_enabled;
                    let board_actions_disabled = !has_tenant;

                    // Default workspace is auto-selected server-side (first membership).
                    // No full-width "Workspace required" CTA — keep the board for real data.
                    let _ = org_count;
                    view! {
                        <header class="board-top">
                            <div class="board-top-copy">
                                <p class="board-kicker">
                                    {if has_tenant {
                                        tenant_label.clone().unwrap_or_else(|| "Workspace".into())
                                    } else {
                                        "Dashboard".into()
                                    }}
                                </p>
                                <h1 class="board-title">{format!("Good to see you, {greeting}")}</h1>
                                <p class="board-sub">
                                    {if has_tenant {
                                        "12-column board with containers, bound widgets, and workspace resources."
                                    } else {
                                        "Create a workspace from the sidebar switcher to edit the board."
                                    }}
                                </p>
                            </div>
                            <div class="board-top-actions">
                                <button type="button" class="secondary-button" class:is-active=move || editing.get()
                                    disabled=board_actions_disabled
                                    on:click=move |_| {
                                        set_editing.update(|v| *v = !*v);
                                        drag_id.set(None);
                                        drop_target.set(None);
                                    }
                                >
                                    {move || if editing.get() { "Done" } else { "Edit board" }}
                                </button>
                                <button type="button" class="secondary-button" on:click=move |_| set_sources_open.set(true)
                                    disabled=move || !http_enabled || board_actions_disabled
                                >
                                    "Resources"
                                </button>
                                <button type="button" class="primary-button" on:click=move |_| set_picker_open.set(true)
                                    disabled=board_actions_disabled
                                >
                                    "Add widget"
                                </button>
                            </div>
                        </header>

                        <Show when=move || editing.get()>
                            <p class="board-edit-hint">
                                "Drag tiles to reorder. Size chips use a 12-column grid (3=¼, 4=⅓, 6=½, 12=full). Add a Row/Stack container to group tiles."
                            </p>
                        </Show>
                        <p class="error-banner" hidden=move || save_error.get().is_none()>
                            {move || save_error.get().unwrap_or_default()}
                        </p>

                        // Widget catalog modal
                        <Show when=move || picker_open.get()>
                            <div
                                class="board-modal-backdrop"
                                role="presentation"
                                on:click=move |_| set_picker_open.set(false)
                                on:wheel=move |e| e.stop_propagation()
                            >
                                <div class="board-modal" role="dialog" aria-modal="true" on:click=move |e| e.stop_propagation()>
                                    <header class="board-modal-head">
                                        <div>
                                            <h2>"Add to board"</h2>
                                            <p>"Widgets, containers, and HTTP panels. Notes and HTTP panels can be added multiple times."</p>
                                        </div>
                                        <button type="button" class="board-modal-close" on:click=move |_| set_picker_open.set(false)>"Close"</button>
                                    </header>
                                    <div class="board-picker-grid board-modal-body">
                                        <article class="board-picker-card">
                                            <div>
                                                <strong>"Row container"</strong>
                                                <p>"Horizontal group for child tiles (12-col)."</p>
                                            </div>
                                            <button type="button" class="primary-button" on:click=move |_| {
                                                let mut next = layout.get_untracked();
                                                next.nodes.push(BoardNode::Container {
                                                    id: format!("c-row-{}", next.total_nodes() + 1),
                                                    kind: BoardContainerKind::Row,
                                                    col_span: 12,
                                                    children: Vec::new(),
                                                });
                                                commit_layout(layout, save_layout, next);
                                                set_picker_open.set(false);
                                            }>"Add row"</button>
                                        </article>
                                        <article class="board-picker-card">
                                            <div>
                                                <strong>"Stack container"</strong>
                                                <p>"Vertical stack for child tiles."</p>
                                            </div>
                                            <button type="button" class="primary-button" on:click=move |_| {
                                                let mut next = layout.get_untracked();
                                                next.nodes.push(BoardNode::Container {
                                                    id: format!("c-stack-{}", next.total_nodes() + 1),
                                                    kind: BoardContainerKind::Stack,
                                                    col_span: 6,
                                                    children: Vec::new(),
                                                });
                                                commit_layout(layout, save_layout, next);
                                                set_picker_open.set(false);
                                            }>"Add stack"</button>
                                        </article>
                                        {
                                            let placed = collect_placed_kinds(&layout.get());
                                            let first_source_base = first_http_source_id.clone();
                                            DashboardWidgetKind::catalog().iter().cloned().map(|kind| {
                                                let multi = kind.allows_multiple();
                                                let already = !multi && placed.contains(kind.as_str());
                                                let kind_add = kind.clone();
                                                let first_source = first_source_base.clone();
                                                view! {
                                                    <article class="board-picker-card" class:is-added=already>
                                                        <div>
                                                            <strong>{kind.label()}</strong>
                                                            <p>{kind.description()}</p>
                                                            <Show when=move || multi>
                                                                <span class="board-picker-badge">"Multiple allowed"</span>
                                                            </Show>
                                                        </div>
                                                        <button type="button" class="primary-button" disabled=already on:click=move |_| {
                                                            if already { return; }
                                                            let mut next = layout.get_untracked();
                                                            let id = next_node_id(kind_add.as_str(), &next);
                                                            let source_id = if kind_add.is_query_bound() {
                                                                first_source.clone()
                                                            } else {
                                                                None
                                                            };
                                                            let mode = kind_add.default_display_mode();
                                                            next.nodes.push(BoardNode::Widget {
                                                                id,
                                                                kind: kind_add.clone(),
                                                                col_span: kind_add.default_span(),
                                                                note_text: if matches!(kind_add, DashboardWidgetKind::Notes) {
                                                                    Some(String::new())
                                                                } else { None },
                                                                source_id,
                                                                bind: WidgetBind::for_display_mode(&mode),
                                                                http_mode: mode,
                                                            });
                                                            commit_layout(layout, save_layout, next);
                                                            set_picker_open.set(false);
                                                        }>
                                                            {if already { "On board" } else { "Add to board" }}
                                                        </button>
                                                    </article>
                                                }
                                            }).collect_view()
                                        }
                                    </div>
                                </div>
                            </div>
                        </Show>

                        {
                            dashboard_resources::resources_queries_modal(
                                sources_open,
                                set_sources_open,
                                http_enabled_flag,
                                grpc_enabled,
                                postgres_enabled,
                                resource_summaries,
                                query_summaries,
                                secrets_for_modal,
                            )
                        }

                        <div class="board-grid board-grid-12" aria-label="Dashboard board">
                            {move || {
                                let nodes = layout.get().nodes;
                                let snap = snapshot.get().or_else(|| board.get().and_then(Result::ok));
                                let notifs = notifications.get();
                                let results = http_results.clone();
                                let q_results = query_results.clone();
                                let q_summaries = query_summaries_for_board.clone();
                                render_node_list(
                                    nodes,
                                    snap,
                                    notifs,
                                    results,
                                    q_results,
                                    q_summaries,
                                    editing,
                                    drag_id,
                                    drop_target,
                                    layout,
                                    save_layout,
                                    dismiss_action,
                                    note_action,
                                )
                            }}
                            <Show when=move || layout.get().nodes.is_empty()>
                                <div class="board-empty-board">
                                    <h2>"Empty board"</h2>
                                    <p>"Add widgets or containers to start designing your workspace."</p>
                                    <button type="button" class="primary-button" on:click=move |_| set_picker_open.set(true)>"Browse catalog"</button>
                                </div>
                            </Show>
                        </div>
                    }.into_any()
                }
            }}
        </div>
    }
}
