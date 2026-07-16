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
use crate::ui::classes::{
    BANNER_ERROR, BOARD_EDIT_HINT, BOARD_EMPTY, BOARD_EMPTY_BOARD, BOARD_EMPTY_BOARD_TITLE,
    BOARD_GRID, BOARD_KICKER, BOARD_MODAL, BOARD_MODAL_BACKDROP, BOARD_MODAL_CLOSE,
    BOARD_MODAL_HEAD, BOARD_MODAL_HEAD_P, BOARD_MODAL_HEAD_TITLE, BOARD_PAGE, BOARD_PICKER_BADGE,
    BOARD_PICKER_CARD, BOARD_PICKER_CARD_ADDED, BOARD_PICKER_CARD_P, BOARD_PICKER_CARD_TITLE,
    BOARD_PICKER_GRID, BOARD_SKELETON, BOARD_SKELETON_BAR, BOARD_SKELETON_GRID, BOARD_SKELETON_SPAN2,
    BOARD_SUB, BOARD_TITLE, BOARD_TOP, BOARD_TOP_ACTIONS, BOARD_TOP_COPY, BTN_PRIMARY,
    BTN_SECONDARY, BTN_SECONDARY_ACTIVE, with_extra,
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
        <div class=BOARD_PAGE>
            {move || match board.get() {
                None => view! {
                    <div class=BOARD_SKELETON aria-busy="true">
                        <div class=BOARD_SKELETON_BAR></div>
                        <div class=BOARD_SKELETON_GRID>
                            <span></span><span></span><span></span><span></span>
                            <span class=BOARD_SKELETON_SPAN2></span><span class=BOARD_SKELETON_SPAN2></span>
                        </div>
                    </div>
                }.into_any(),
                Some(Err(error)) => view! {
                    <section class=BOARD_EMPTY>
                        <p class=BANNER_ERROR>{server_error_text(error)}</p>
                    </section>
                }.into_any(),
                Some(Ok(_)) => {
                    let data = snapshot.get().or_else(|| board.get().and_then(Result::ok));
                    let Some(data) = data else {
                        return view! { <div class=BOARD_SKELETON aria-busy="true"></div> }.into_any();
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
                        <header class=BOARD_TOP>
                            <div class=BOARD_TOP_COPY>
                                <p class=BOARD_KICKER>
                                    {if has_tenant {
                                        tenant_label.clone().unwrap_or_else(|| "Workspace".into())
                                    } else {
                                        "Dashboard".into()
                                    }}
                                </p>
                                <h1 class=BOARD_TITLE>{format!("Good to see you, {greeting}")}</h1>
                                <p class=BOARD_SUB>
                                    {if has_tenant {
                                        "12-column board with containers, bound widgets, and workspace resources."
                                    } else {
                                        "Create a workspace from the sidebar switcher to edit the board."
                                    }}
                                </p>
                            </div>
                            <div class=BOARD_TOP_ACTIONS>
                                <button type="button"
                                    class=move || if editing.get() { BTN_SECONDARY_ACTIVE } else { BTN_SECONDARY }
                                    disabled=board_actions_disabled
                                    on:click=move |_| {
                                        set_editing.update(|v| *v = !*v);
                                        drag_id.set(None);
                                        drop_target.set(None);
                                    }
                                >
                                    {move || if editing.get() { "Done" } else { "Edit board" }}
                                </button>
                                <button type="button" class=BTN_SECONDARY on:click=move |_| set_sources_open.set(true)
                                    disabled=move || !http_enabled || board_actions_disabled
                                >
                                    "Resources"
                                </button>
                                <button type="button" class=BTN_PRIMARY on:click=move |_| set_picker_open.set(true)
                                    disabled=board_actions_disabled
                                >
                                    "Add widget"
                                </button>
                            </div>
                        </header>

                        <Show when=move || editing.get()>
                            <p class=BOARD_EDIT_HINT>
                                "Drag tiles to reorder. Size chips use a 12-column grid (3=¼, 4=⅓, 6=½, 12=full). Add a Row/Stack container to group tiles."
                            </p>
                        </Show>
                        <p class=BANNER_ERROR hidden=move || save_error.get().is_none()>
                            {move || save_error.get().unwrap_or_default()}
                        </p>

                        // Widget catalog modal
                        <Show when=move || picker_open.get()>
                            <div
                                class=BOARD_MODAL_BACKDROP
                                role="presentation"
                                on:click=move |_| set_picker_open.set(false)
                                on:wheel=move |e| e.stop_propagation()
                            >
                                <div class=BOARD_MODAL role="dialog" aria-modal="true" on:click=move |e| e.stop_propagation()>
                                    <header class=BOARD_MODAL_HEAD>
                                        <div>
                                            <h2 class=BOARD_MODAL_HEAD_TITLE>"Add to board"</h2>
                                            <p class=BOARD_MODAL_HEAD_P>"Widgets, containers, and HTTP panels. Notes and HTTP panels can be added multiple times."</p>
                                        </div>
                                        <button type="button" class=BOARD_MODAL_CLOSE on:click=move |_| set_picker_open.set(false)>"Close"</button>
                                    </header>
                                    <div class=BOARD_PICKER_GRID>
                                        <article class=BOARD_PICKER_CARD>
                                            <div>
                                                <strong class=BOARD_PICKER_CARD_TITLE>"Row container"</strong>
                                                <p class=BOARD_PICKER_CARD_P>"Horizontal group for child tiles (12-col)."</p>
                                            </div>
                                            <button type="button" class=BTN_PRIMARY on:click=move |_| {
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
                                        <article class=BOARD_PICKER_CARD>
                                            <div>
                                                <strong class=BOARD_PICKER_CARD_TITLE>"Stack container"</strong>
                                                <p class=BOARD_PICKER_CARD_P>"Vertical stack for child tiles."</p>
                                            </div>
                                            <button type="button" class=BTN_PRIMARY on:click=move |_| {
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
                                                let card_class = if already {
                                                    with_extra(BOARD_PICKER_CARD, Some(BOARD_PICKER_CARD_ADDED))
                                                } else {
                                                    BOARD_PICKER_CARD.to_owned()
                                                };
                                                view! {
                                                    <article class=card_class>
                                                        <div>
                                                            <strong class=BOARD_PICKER_CARD_TITLE>{kind.label()}</strong>
                                                            <p class=BOARD_PICKER_CARD_P>{kind.description()}</p>
                                                            <Show when=move || multi>
                                                                <span class=BOARD_PICKER_BADGE>"Multiple allowed"</span>
                                                            </Show>
                                                        </div>
                                                        <button type="button" class=BTN_PRIMARY disabled=already on:click=move |_| {
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

                        <div class=BOARD_GRID aria-label="Dashboard board">
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
                                <div class=BOARD_EMPTY_BOARD>
                                    <h2 class=BOARD_EMPTY_BOARD_TITLE>"Empty board"</h2>
                                    <p>"Add widgets or containers to start designing your workspace."</p>
                                    <button type="button" class=BTN_PRIMARY on:click=move |_| set_picker_open.set(true)>"Browse catalog"</button>
                                </div>
                            </Show>
                        </div>
                    }.into_any()
                }
            }}
        </div>
    }
}
