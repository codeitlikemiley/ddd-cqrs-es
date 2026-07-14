//! Extensible dashboard board (layout v2, containers, HTTP sources).

use super::resources as dashboard_resources;

use crate::contracts::{
    BoardContainerKind, BoardNode, DashboardLayout, DashboardNotification, DashboardSnapshot,
    DashboardWidgetKind, HttpDisplayMode, HttpQueryResult, QueryResult, QuerySummary, WidgetBind,
};
use leptos::prelude::*;
#[cfg(feature = "hydrate")]
use leptos::task::spawn_local;
#[cfg(feature = "hydrate")]
use wasm_bindgen::JsCast;

use crate::app::helpers::server_error_text;
use crate::app::{
    browser_load, get_dashboard_snapshot, DismissDashboardNotification, SaveDashboardLayout,
    UpdateDashboardNote,
};

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
        Some(Err(error)) => set_save_error.set(Some(error.to_string())),
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

fn collect_placed_kinds(layout: &DashboardLayout) -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    fn walk(nodes: &[BoardNode], set: &mut std::collections::HashSet<String>) {
        for node in nodes {
            match node {
                BoardNode::Widget { kind, .. } => {
                    set.insert(kind.as_str().to_owned());
                }
                BoardNode::Container { children, .. } => walk(children, set),
            }
        }
    }
    walk(&layout.nodes, &mut set);
    set
}

fn next_node_id(prefix: &str, layout: &DashboardLayout) -> String {
    let mut n = layout.total_nodes().saturating_add(1);
    loop {
        let candidate = format!("{prefix}-{n}");
        if !id_exists(&layout.nodes, &candidate) {
            return candidate;
        }
        n += 1;
    }
}

fn id_exists(nodes: &[BoardNode], id: &str) -> bool {
    for node in nodes {
        if node.id() == id {
            return true;
        }
        if let BoardNode::Container { children, .. } = node
            && id_exists(children, id)
        {
            return true;
        }
    }
    false
}

fn remove_node(nodes: &mut Vec<BoardNode>, id: &str) -> bool {
    if let Some(idx) = nodes.iter().position(|n| n.id() == id) {
        nodes.remove(idx);
        return true;
    }
    for node in nodes.iter_mut() {
        if let BoardNode::Container { children, .. } = node
            && remove_node(children, id)
        {
            return true;
        }
    }
    false
}

/// Reorder two nodes that share the same parent list (root or container children).
fn reorder_siblings(nodes: &mut Vec<BoardNode>, from_id: &str, to_id: &str) -> bool {
    if from_id == to_id {
        return false;
    }
    if let (Some(from_idx), Some(to_idx)) = (
        nodes.iter().position(|n| n.id() == from_id),
        nodes.iter().position(|n| n.id() == to_id),
    ) {
        let mut to_idx = to_idx;
        let item = nodes.remove(from_idx);
        if from_idx < to_idx {
            to_idx -= 1;
        }
        nodes.insert(to_idx.min(nodes.len()), item);
        return true;
    }
    for node in nodes.iter_mut() {
        if let BoardNode::Container { children, .. } = node
            && reorder_siblings(children, from_id, to_id)
        {
            return true;
        }
    }
    false
}

fn set_span_by_id(nodes: &mut [BoardNode], id: &str, span: u8) -> bool {
    for node in nodes.iter_mut() {
        if node.id() == id {
            node.set_col_span(span);
            return true;
        }
        if let BoardNode::Container { children, .. } = node
            && set_span_by_id(children, id, span)
        {
            return true;
        }
    }
    false
}

fn find_col_span(nodes: &[BoardNode], id: &str) -> Option<u8> {
    for node in nodes {
        if node.id() == id {
            return Some(node.col_span());
        }
        if let BoardNode::Container { children, .. } = node
            && let Some(span) = find_col_span(children, id)
        {
            return Some(span);
        }
    }
    None
}

fn find_node<'a>(nodes: &'a [BoardNode], id: &str) -> Option<&'a BoardNode> {
    for node in nodes {
        if node.id() == id {
            return Some(node);
        }
        if let BoardNode::Container { children, .. } = node
            && let Some(found) = find_node(children, id)
        {
            return Some(found);
        }
    }
    None
}

fn commit_layout(layout: RwSignal<DashboardLayout>, save_layout: ServerAction<SaveDashboardLayout>, next: DashboardLayout) {
    // Write signal first so fine-grained attrs (data-span / chip active) update immediately.
    layout.set(next.clone());
    save_layout.dispatch(SaveDashboardLayout { layout: next });
}

fn render_node_list(
    nodes: Vec<BoardNode>,
    snap: Option<DashboardSnapshot>,
    notifs: Vec<DashboardNotification>,
    http_results: Vec<HttpQueryResult>,
    query_results: Vec<QueryResult>,
    query_summaries: Vec<QuerySummary>,
    editing: ReadSignal<bool>,
    drag_id: RwSignal<Option<String>>,
    drop_target: RwSignal<Option<String>>,
    layout: RwSignal<DashboardLayout>,
    save_layout: ServerAction<SaveDashboardLayout>,
    dismiss_action: ServerAction<DismissDashboardNotification>,
    note_action: ServerAction<UpdateDashboardNote>,
) -> AnyView {
    // Key by id so reorder moves DOM nodes instead of patching in place by index
    // (index patching left stale data-span / body content until full page reload).
    nodes
        .into_iter()
        .map(|node| {
            let key = node.id().to_owned();
            view! {
                <div class="board-node-slot" data-node-id=key>
                    {render_node(
                        node,
                        snap.clone(),
                        notifs.clone(),
                        http_results.clone(),
                        query_results.clone(),
                        query_summaries.clone(),
                        editing,
                        drag_id,
                        drop_target,
                        layout,
                        save_layout,
                        dismiss_action,
                        note_action,
                    )}
                </div>
            }
        })
        .collect_view()
        .into_any()
}

fn render_node(
    node: BoardNode,
    snap: Option<DashboardSnapshot>,
    notifs: Vec<DashboardNotification>,
    http_results: Vec<HttpQueryResult>,
    query_results: Vec<QueryResult>,
    query_summaries: Vec<QuerySummary>,
    editing: ReadSignal<bool>,
    drag_id: RwSignal<Option<String>>,
    drop_target: RwSignal<Option<String>>,
    layout: RwSignal<DashboardLayout>,
    save_layout: ServerAction<SaveDashboardLayout>,
    dismiss_action: ServerAction<DismissDashboardNotification>,
    note_action: ServerAction<UpdateDashboardNote>,
) -> AnyView {
    match node {
        BoardNode::Container {
            id,
            kind,
            col_span: initial_span,
            children,
        } => {
            let id_span = id.clone();
            let id_ds = id.clone();
            let id_dover = id.clone();
            let id_ddrop = id.clone();
            let id_remove = id.clone();
            let id_chips = id.clone();
            let parent_id = id.clone();
            let parent_id_empty = id.clone();
            let kind_label = kind.label();
            let is_row = matches!(kind, BoardContainerKind::Row);
            let span_attr = {
                let id_span = id_span.clone();
                move || {
                    find_col_span(&layout.get().nodes, &id_span)
                        .unwrap_or(initial_span)
                        .to_string()
                }
            };
            let grid_style = {
                let id_span = id_span.clone();
                move || {
                    let span = find_col_span(&layout.get().nodes, &id_span).unwrap_or(initial_span);
                    format!("grid-column: span {span}")
                }
            };
            view! {
                <section
                    class="board-container"
                    class:is-row=is_row
                    class:is-stack=!is_row
                    class:is-editing=move || editing.get()
                    // Reactive: reads layout signal so width updates without full remount.
                    data-span=span_attr
                    style=grid_style
                    // HTML draggable is enumerated ("true"/"false"), not a boolean attr.
                    draggable=move || if editing.get() { "true" } else { "false" }
                    on:dragstart=move |event| {
                        if !editing.get_untracked() {
                            return;
                        }
                        drag_id.set(Some(id_ds.clone()));
                        #[cfg(feature = "hydrate")]
                        {
                            if let Some(drag) = event.dyn_ref::<web_sys::DragEvent>() {
                                if let Some(dt) = drag.data_transfer() {
                                    let _ = dt.set_data("text/plain", &id_ds);
                                    dt.set_effect_allowed("move");
                                }
                            }
                        }
                        #[cfg(not(feature = "hydrate"))]
                        {
                            let _ = event;
                        }
                    }
                    on:dragover=move |e| {
                        if editing.get_untracked() {
                            e.prevent_default();
                            #[cfg(feature = "hydrate")]
                            if let Some(drag) = e.dyn_ref::<web_sys::DragEvent>() {
                                if let Some(dt) = drag.data_transfer() {
                                    let _ = dt.set_drop_effect("move");
                                }
                            }
                            drop_target.set(Some(id_dover.clone()));
                        }
                    }
                    on:drop=move |e| {
                        e.prevent_default();
                        if !editing.get_untracked() {
                            return;
                        }
                        let from = drag_id.get_untracked();
                        drag_id.set(None);
                        drop_target.set(None);
                        let Some(from_id) = from else {
                            return;
                        };
                        let mut next = layout.get_untracked();
                        if reorder_siblings(&mut next.nodes, &from_id, &id_ddrop) {
                            commit_layout(layout, save_layout, next);
                        }
                    }
                    on:dragend=move |_| {
                        drag_id.set(None);
                        drop_target.set(None);
                    }
                >
                    <header class="board-container-head">
                        <span class="board-tile-kicker">{kind_label}</span>
                        <Show when=move || editing.get()>
                            <div
                                class="board-tile-controls"
                                draggable="false"
                                on:mousedown=move |e| e.stop_propagation()
                                on:dragstart=move |e| {
                                    e.prevent_default();
                                    e.stop_propagation();
                                }
                            >
                                {span_chips(id_chips.clone(), layout, save_layout)}
                                <button type="button" class="board-tile-remove" aria-label="Remove container" on:click={
                                    let id_remove = id_remove.clone();
                                    move |_| {
                                        let mut next = layout.get_untracked();
                                        if remove_node(&mut next.nodes, &id_remove) {
                                            commit_layout(layout, save_layout, next);
                                        }
                                    }
                                }>"×"</button>
                            </div>
                        </Show>
                    </header>
                    <div class="board-container-body" class:is-row=is_row>
                        {
                            let empty = children.is_empty();
                            // Re-render children from layout so nested reorder/span stay live.
                            let child_view = {
                                let parent_id = parent_id.clone();
                                move || {
                                    let lay = layout.get();
                                    let kids = find_node(&lay.nodes, &parent_id)
                                        .and_then(|n| match n {
                                            BoardNode::Container { children, .. } => Some(children.clone()),
                                            _ => None,
                                        })
                                        .unwrap_or_default();
                                    render_node_list(
                                        kids,
                                        snap.clone(),
                                        notifs.clone(),
                                        http_results.clone(),
                                        query_results.clone(),
                                        query_summaries.clone(),
                                        editing,
                                        drag_id,
                                        drop_target,
                                        layout,
                                        save_layout,
                                        dismiss_action,
                                        note_action,
                                    )
                                }
                            };
                            view! {
                                {child_view}
                                <Show when=move || {
                                    editing.get()
                                        && find_node(&layout.get().nodes, &parent_id_empty)
                                            .and_then(|n| match n {
                                                BoardNode::Container { children, .. } => Some(children.is_empty()),
                                                _ => None,
                                            })
                                            .unwrap_or(empty)
                                }>
                                    <p class="board-muted">"Empty container — add tiles at the root level for now; nest by grouping with rows."</p>
                                </Show>
                            }
                        }
                    </div>
                </section>
            }
            .into_any()
        }
        BoardNode::Widget {
            id,
            kind,
            col_span: initial_span,
            note_text,
            source_id,
            bind,
            http_mode,
        } => {
            let id_cls = id.clone();
            let id_span = id.clone();
            let id_ds = id.clone();
            let id_dover = id.clone();
            let id_ddrop = id.clone();
            let id_remove = id.clone();
            let id_chips = id.clone();
            let id_bind = id.clone();
            let kind_label = kind.label();
            let is_bound = kind.is_query_bound();
            let span_attr = {
                let id_span = id_span.clone();
                move || {
                    find_col_span(&layout.get().nodes, &id_span)
                        .unwrap_or(initial_span)
                        .to_string()
                }
            };
            let grid_style = {
                let id_span = id_span.clone();
                move || {
                    let span = find_col_span(&layout.get().nodes, &id_span).unwrap_or(initial_span);
                    format!("grid-column: span {span}")
                }
            };
            let body = render_widget_body(
                kind.clone(),
                snap,
                notifs,
                note_text.unwrap_or_default(),
                id.clone(),
                source_id.clone(),
                bind.clone(),
                http_mode.clone(),
                http_results,
                query_results,
                dismiss_action,
                note_action,
            );
            let source_id_edit = source_id.clone();
            let bind_edit = bind.clone();
            let mode_edit = http_mode.clone();
            let queries_edit = query_summaries.clone();
            view! {
                <article
                    class="board-tile"
                    data-span=span_attr
                    style=grid_style
                    class:is-editing=move || editing.get()
                    class:is-drop-target=move || {
                        drop_target.get().as_deref() == Some(id_cls.as_str())
                            && drag_id.get().as_deref() != Some(id_cls.as_str())
                    }
                    draggable=move || if editing.get() { "true" } else { "false" }
                    on:dragstart=move |event| {
                        if !editing.get_untracked() {
                            return;
                        }
                        drag_id.set(Some(id_ds.clone()));
                        #[cfg(feature = "hydrate")]
                        {
                            if let Some(drag) = event.dyn_ref::<web_sys::DragEvent>() {
                                if let Some(dt) = drag.data_transfer() {
                                    let _ = dt.set_data("text/plain", &id_ds);
                                    dt.set_effect_allowed("move");
                                }
                            }
                        }
                        #[cfg(not(feature = "hydrate"))]
                        {
                            let _ = event;
                        }
                    }
                    on:dragover=move |e| {
                        if editing.get_untracked() {
                            e.prevent_default();
                            #[cfg(feature = "hydrate")]
                            if let Some(drag) = e.dyn_ref::<web_sys::DragEvent>() {
                                if let Some(dt) = drag.data_transfer() {
                                    let _ = dt.set_drop_effect("move");
                                }
                            }
                            drop_target.set(Some(id_dover.clone()));
                        }
                    }
                    on:drop=move |e| {
                        e.prevent_default();
                        if !editing.get_untracked() {
                            return;
                        }
                        let from = drag_id.get_untracked();
                        drag_id.set(None);
                        drop_target.set(None);
                        let Some(from_id) = from else {
                            return;
                        };
                        let mut next = layout.get_untracked();
                        if reorder_siblings(&mut next.nodes, &from_id, &id_ddrop) {
                            commit_layout(layout, save_layout, next);
                        }
                    }
                    on:dragend=move |_| {
                        drag_id.set(None);
                        drop_target.set(None);
                    }
                >
                    <header class="board-tile-head">
                        <div class="board-tile-head-main">
                            <Show when=move || editing.get()>
                                <span class="board-drag-handle" aria-hidden="true">"⠿"</span>
                            </Show>
                            <p class="board-tile-kicker">{kind_label}</p>
                        </div>
                        <Show when=move || editing.get()>
                            {
                                let id_chips = id_chips.clone();
                                let id_remove = id_remove.clone();
                                view! {
                                    <div
                                        class="board-tile-controls"
                                        draggable="false"
                                        on:mousedown=move |e| e.stop_propagation()
                                        on:dragstart=move |e| {
                                            e.prevent_default();
                                            e.stop_propagation();
                                        }
                                    >
                                        {span_chips(id_chips, layout, save_layout)}
                                        <button type="button" class="board-tile-remove" aria-label="Remove" on:click=move |_| {
                                            let mut next = layout.get_untracked();
                                            if remove_node(&mut next.nodes, &id_remove) {
                                                commit_layout(layout, save_layout, next);
                                            }
                                        }>
                                            <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
                                                <path fill="currentColor" d="M3.72 3.72a.75.75 0 0 1 1.06 0L8 6.94l3.22-3.22a.75.75 0 1 1 1.06 1.06L9.06 8l3.22 3.22a.75.75 0 1 1-1.06 1.06L8 9.06l-3.22 3.22a.75.75 0 0 1-1.06-1.06L6.94 8 3.72 4.78a.75.75 0 0 1 0-1.06Z"/>
                                            </svg>
                                        </button>
                                    </div>
                                }
                            }
                        </Show>
                    </header>
                    <Show when=move || editing.get() && is_bound>
                        {
                            bind_fields_editor(
                                id_bind.clone(),
                                source_id_edit.clone(),
                                bind_edit.clone(),
                                mode_edit.clone(),
                                queries_edit.clone(),
                                layout,
                                save_layout,
                            )
                        }
                    </Show>
                    <div class="board-tile-body" class:is-dimmed=move || editing.get() && !is_bound>{body}</div>
                </article>
            }
            .into_any()
        }
    }
}

fn update_widget_bind(
    layout: RwSignal<DashboardLayout>,
    save_layout: ServerAction<SaveDashboardLayout>,
    widget_id: &str,
    mutator: impl FnOnce(&mut Option<String>, &mut WidgetBind, &mut HttpDisplayMode),
) {
    let mut next = layout.get_untracked();
    if let Some(BoardNode::Widget {
        source_id,
        bind,
        http_mode,
        ..
    }) = next.find_widget_mut(widget_id)
    {
        mutator(source_id, bind, http_mode);
        commit_layout(layout, save_layout, next);
    }
}

fn bind_fields_editor(
    widget_id: String,
    source_id: Option<String>,
    bind: WidgetBind,
    http_mode: HttpDisplayMode,
    query_summaries: Vec<QuerySummary>,
    layout: RwSignal<DashboardLayout>,
    save_layout: ServerAction<SaveDashboardLayout>,
) -> AnyView {
    let selected = source_id.clone().unwrap_or_default();
    let value_path = bind.value_path.clone().unwrap_or_default();
    let label_path = bind.label_path.clone().unwrap_or_default();
    let title_path = bind.title_path.clone().unwrap_or_default();
    let subtitle_path = bind.subtitle_path.clone().unwrap_or_default();
    let items_path = bind.items_path.clone().unwrap_or_default();
    let meta_path = bind.meta_path.clone().unwrap_or_default();
    let mode = http_mode.clone();
    let queries = query_summaries;
    let wid = widget_id.clone();

    view! {
        <div
            class="board-bind-editor"
            draggable="false"
            on:mousedown=move |e| e.stop_propagation()
            on:dragstart=move |e| {
                e.prevent_default();
                e.stop_propagation();
            }
        >
            <label class="board-bind-field">
                <span>"Query"</span>
                <select
                    class="auth-input"
                    prop:value=selected.clone()
                    on:change={
                        let wid = wid.clone();
                        move |e| {
                            let v = event_target_value_board(&e);
                            update_widget_bind(layout, save_layout, &wid, |source_id, _, _| {
                                *source_id = if v.is_empty() { None } else { Some(v) };
                            });
                        }
                    }
                >
                    <option value="">"— select query —"</option>
                    {queries.into_iter().map(|q| {
                        let id = q.id.clone();
                        let label = format!("{} · {}", q.name, q.detail);
                        view! { <option value=id>{label}</option> }
                    }).collect_view()}
                </select>
            </label>
            <label class="board-bind-field">
                <span>"Display"</span>
                <select
                    class="auth-input"
                    prop:value=match mode {
                        HttpDisplayMode::Metric => "metric",
                        HttpDisplayMode::List => "list",
                        HttpDisplayMode::Table => "table",
                    }
                    on:change={
                        let wid = wid.clone();
                        move |e| {
                            let v = event_target_value_board(&e);
                            update_widget_bind(layout, save_layout, &wid, |_, bind, mode| {
                                *mode = match v.as_str() {
                                    "metric" => HttpDisplayMode::Metric,
                                    "table" => HttpDisplayMode::Table,
                                    _ => HttpDisplayMode::List,
                                };
                                if bind.title_path.is_none() && bind.value_path.is_none() {
                                    *bind = WidgetBind::for_display_mode(mode);
                                }
                            });
                        }
                    }
                >
                    <option value="metric">"Metric"</option>
                    <option value="list">"List"</option>
                    <option value="table">"Table"</option>
                </select>
            </label>
            <label class="board-bind-field">
                <span>"Items path"</span>
                <input class="auth-input" prop:value=items_path placeholder="e.g. data.items"
                    on:change={
                        let wid = wid.clone();
                        move |e| {
                            let v = event_target_value_board(&e);
                            update_widget_bind(layout, save_layout, &wid, |_, bind, _| {
                                bind.items_path = if v.trim().is_empty() { None } else { Some(v) };
                            });
                        }
                    }
                />
            </label>
            <div class="board-bind-row">
                <label class="board-bind-field">
                    <span>"Value / title path"</span>
                    <input class="auth-input" prop:value=if matches!(mode, HttpDisplayMode::Metric) { value_path.clone() } else { title_path.clone() }
                        placeholder=if matches!(mode, HttpDisplayMode::Metric) { "value" } else { "name" }
                        on:change={
                            let wid = wid.clone();
                            let is_metric = matches!(mode, HttpDisplayMode::Metric);
                            move |e| {
                                let v = event_target_value_board(&e);
                                update_widget_bind(layout, save_layout, &wid, |_, bind, _| {
                                    if is_metric {
                                        bind.value_path = if v.trim().is_empty() { None } else { Some(v) };
                                    } else {
                                        bind.title_path = if v.trim().is_empty() { None } else { Some(v) };
                                    }
                                });
                            }
                        }
                    />
                </label>
                <label class="board-bind-field">
                    <span>"Label / subtitle path"</span>
                    <input class="auth-input" prop:value=if matches!(mode, HttpDisplayMode::Metric) { label_path } else { subtitle_path }
                        placeholder=if matches!(mode, HttpDisplayMode::Metric) { "label" } else { "id" }
                        on:change={
                            let wid = wid.clone();
                            let is_metric = matches!(mode, HttpDisplayMode::Metric);
                            move |e| {
                                let v = event_target_value_board(&e);
                                update_widget_bind(layout, save_layout, &wid, |_, bind, _| {
                                    if is_metric {
                                        bind.label_path = if v.trim().is_empty() { None } else { Some(v) };
                                    } else {
                                        bind.subtitle_path = if v.trim().is_empty() { None } else { Some(v) };
                                    }
                                });
                            }
                        }
                    />
                </label>
            </div>
            <label class="board-bind-field">
                <span>"Meta path"</span>
                <input class="auth-input" prop:value=meta_path placeholder="optional"
                    on:change={
                        let wid = wid.clone();
                        move |e| {
                            let v = event_target_value_board(&e);
                            update_widget_bind(layout, save_layout, &wid, |_, bind, _| {
                                bind.meta_path = if v.trim().is_empty() { None } else { Some(v) };
                            });
                        }
                    }
                />
            </label>
            <p class="board-muted board-bind-hint">"Bind paths are dotted JSON paths relative to items path (or root). Table auto-detects columns when empty."</p>
        </div>
    }
    .into_any()
}

fn event_target_value_board(event: &leptos::ev::Event) -> String {
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
            .unwrap_or_default();
    }
    #[cfg(not(feature = "hydrate"))]
    {
        let _ = event;
        String::new()
    }
}

fn span_chips(
    id: String,
    layout: RwSignal<DashboardLayout>,
    save_layout: ServerAction<SaveDashboardLayout>,
) -> AnyView {
    let presets = [(3u8, "¼"), (4, "⅓"), (6, "½"), (12, "Full")];
    view! {
        <div class="board-span-group" role="group" aria-label="Width">
            {presets.into_iter().map(|(size, label)| {
                let id = id.clone();
                let id_active = id.clone();
                view! {
                    <button
                        type="button"
                        class="board-span-chip"
                        // Reactive active state — was frozen at first render before.
                        class:is-active=move || {
                            find_col_span(&layout.get().nodes, &id_active).unwrap_or(0) == size
                        }
                        on:click=move |ev| {
                            ev.stop_propagation();
                            let mut next = layout.get_untracked();
                            if set_span_by_id(&mut next.nodes, &id, size) {
                                commit_layout(layout, save_layout, next);
                            }
                        }
                    >{label}</button>
                }
            }).collect_view()}
        </div>
    }
    .into_any()
}

fn render_widget_body(
    kind: DashboardWidgetKind,
    data: Option<DashboardSnapshot>,
    notifications: Vec<DashboardNotification>,
    note_text: String,
    widget_id: String,
    source_id: Option<String>,
    bind: WidgetBind,
    http_mode: HttpDisplayMode,
    http_results: Vec<HttpQueryResult>,
    query_results: Vec<QueryResult>,
    dismiss_action: ServerAction<DismissDashboardNotification>,
    note_action: ServerAction<UpdateDashboardNote>,
) -> AnyView {
    if kind.is_query_bound() {
        return render_bound_widget(source_id, bind, http_mode, &http_results, &query_results);
    }

    let Some(data) = data else {
        return view! { <p class="board-muted">"Loading…"</p> }.into_any();
    };

    match kind {
        DashboardWidgetKind::MetricSession => view! {
            <div class="board-metric">
                <strong class="board-metric-value">
                    <span class="board-pulse" aria-hidden="true"></span>"Verified"
                </strong>
                <span class="board-metric-meta">{data.assurance.to_uppercase()}</span>
            </div>
        }
        .into_any(),
        DashboardWidgetKind::MetricDevices => view! {
            <div class="board-metric">
                <strong class="board-metric-value board-metric-number">{data.active_session_count.to_string()}</strong>
                <span class="board-metric-meta">"signed-in sessions"</span>
            </div>
        }
        .into_any(),
        DashboardWidgetKind::MetricOrgs => view! {
            <div class="board-metric">
                <strong class="board-metric-value board-metric-number">{data.organization_count.to_string()}</strong>
                <span class="board-metric-meta">"workspaces"</span>
            </div>
        }
        .into_any(),
        DashboardWidgetKind::MetricSecurity => view! {
            <div class="board-metric">
                <strong class="board-metric-value board-metric-number">{format!("{}%", data.security_score)}</strong>
                <span class="board-metric-meta">{if data.totp_enrolled { "MFA on" } else { "MFA off" }}</span>
                <div class="board-score-bar" aria-hidden="true"><span style=format!("width:{}%", data.security_score)></span></div>
            </div>
        }
        .into_any(),
        DashboardWidgetKind::Activity => {
            if data.activity.is_empty() {
                view! {
                    <div class="board-empty-tile">
                        <p>{if data.has_tenant { "No audit events yet." } else { "Select an organization to stream activity." }}</p>
                    </div>
                }
                .into_any()
            } else {
                view! {
                    <ul class="board-feed">
                        {data.activity.into_iter().take(8).map(|event| view! {
                            <li class="board-feed-item">
                                <span class="board-feed-dot" data-outcome=event.outcome.clone()></span>
                                <div class="board-feed-copy">
                                    <strong>{event.action}</strong>
                                    <span>{format!("{} · {}", event.outcome, relative_ms(event.recorded_at_ms))}</span>
                                </div>
                            </li>
                        }).collect_view()}
                    </ul>
                }
                .into_any()
            }
        }
        DashboardWidgetKind::Notifications => {
            if notifications.is_empty() {
                view! { <div class="board-empty-tile"><p>"You're caught up."</p></div> }.into_any()
            } else {
                view! {
                    <ul class="board-notif-list">
                        {notifications.into_iter().map(|item| {
                            let id = item.id.clone();
                            view! {
                                <li class="board-notif" data-level=item.level.clone()>
                                    <div class="board-notif-copy">
                                        <strong>{item.title}</strong>
                                        <p>{item.body}</p>
                                        <span class="board-notif-time">{relative_ms(item.created_at_ms)}</span>
                                    </div>
                                    <button type="button" class="board-notif-dismiss" on:click=move |_| {
                                        dismiss_action.dispatch(DismissDashboardNotification { notification_id: id.clone() });
                                    }>"Dismiss"</button>
                                </li>
                            }
                        }).collect_view()}
                    </ul>
                }
                .into_any()
            }
        }
        DashboardWidgetKind::Sessions => view! {
            <ul class="board-list">
                {data.sessions.into_iter().take(6).map(|session| view! {
                    <li class="board-list-row">
                        <div>
                            <strong>{if session.current { "This browser" } else { "Other session" }}</strong>
                            <span class="board-list-meta">{format!("{} · {}", session.assurance.to_uppercase(), relative_ms(session.expires_at_ms))}</span>
                        </div>
                        <span class="board-pill" class:is-live=session.current>{if session.current { "Live" } else { "Active" }}</span>
                    </li>
                }).collect_view()}
            </ul>
            <a class="board-inline-link" href="/account/sessions">"Manage sessions"</a>
        }
        .into_any(),
        DashboardWidgetKind::Organizations => {
            if data.organizations.is_empty() {
                view! { <div class="board-empty-tile"><p>"No workspaces yet."</p><a class="board-inline-link" href="/organizations">"Create one"</a></div> }.into_any()
            } else {
                let active = data.tenant_label.clone();
                view! {
                    <ul class="board-list">
                        {data.organizations.into_iter().take(6).map(|org| {
                            let is_active = active.as_ref().is_some_and(|t| t == &org.organization_id);
                            view! {
                                <li class="board-list-row">
                                    <div class="board-list-grow">
                                        <strong>{org.name}</strong>
                                        <span class="board-list-meta">{org.current_user_role}</span>
                                    </div>
                                    <span class="board-pill" class:is-live=is_active>{if is_active { "Active" } else { "Joined" }}</span>
                                </li>
                            }
                        }).collect_view()}
                    </ul>
                    <a class="board-inline-link" href="/organizations">"All organizations"</a>
                }
                .into_any()
            }
        }
        DashboardWidgetKind::SecurityPosture => view! {
            <div class="board-security">
                <div class="board-security-score">
                    <strong>{format!("{}%", data.security_score)}</strong><span>"posture"</span>
                </div>
                <ul class="board-checklist">
                    <li class:is-done=data.totp_enrolled>{if data.totp_enrolled { "Authenticator enrolled" } else { "Enroll authenticator" }}</li>
                    <li class:is-done=(data.recovery_codes_remaining > 0)>{format!("{} recovery codes left", data.recovery_codes_remaining)}</li>
                    <li class:is-done=(data.active_session_count <= 3)>{format!("{} active sessions", data.active_session_count)}</li>
                </ul>
                <div class="board-inline-links">
                    <a class="board-inline-link" href="/account/mfa">"MFA"</a>
                    <a class="board-inline-link" href="/account/passkeys">"Passkeys"</a>
                    <a class="board-inline-link" href="/account/sessions">"Sessions"</a>
                </div>
            </div>
        }
        .into_any(),
        DashboardWidgetKind::Notes => {
            let widget_id_save = widget_id.clone();
            view! {
                <div class="board-notes">
                    <textarea class="board-notes-input" rows="5" maxlength="2000"
                        placeholder="Scratch pad — only you can see this."
                        prop:value=note_text
                        on:blur=move |event| {
                            note_action.dispatch(UpdateDashboardNote {
                                widget_id: widget_id_save.clone(),
                                text: event_target_value(&event),
                            });
                        }
                    />
                    <p class="board-muted">"Saves when you leave the field."</p>
                </div>
            }
            .into_any()
        }
        DashboardWidgetKind::Checklist => view! {
            <ul class="board-checklist board-checklist-lg">
                <li class:is-done=data.email.is_some()><a href="/account/profile">"Complete profile"</a></li>
                <li class:is-done=(data.organization_count > 0)><a href="/organizations">"Create or join an organization"</a></li>
                <li class:is-done=data.has_tenant><a href="/organizations">"Select an active tenant"</a></li>
                <li class:is-done=data.totp_enrolled><a href="/account/mfa">"Turn on multi-factor auth"</a></li>
            </ul>
        }
        .into_any(),
        DashboardWidgetKind::HttpPanel
        | DashboardWidgetKind::BoundMetric
        | DashboardWidgetKind::BoundList
        | DashboardWidgetKind::BoundTable => {
            view! { <p class="board-muted">"Query-bound widget"</p> }.into_any()
        }
    }
}

fn render_bound_widget(
    source_id: Option<String>,
    bind: WidgetBind,
    http_mode: HttpDisplayMode,
    http_results: &[HttpQueryResult],
    query_results: &[QueryResult],
) -> AnyView {
    let Some(qid) = source_id.filter(|s| !s.is_empty()) else {
        return view! {
            <div class="board-empty-tile">
                <p>"Edit board → bind a query on this tile (Resources → Query)."</p>
            </div>
        }
        .into_any();
    };

    // Prefer QueryResult (raw/transformed); fall back to legacy HttpQueryResult.
    let (ok, error, data_json) = if let Some(r) = query_results.iter().find(|r| r.query_id == qid) {
        (r.ok, r.error.clone(), r.data_json.clone())
    } else if let Some(r) = http_results.iter().find(|r| r.source_id == qid) {
        (r.ok, r.error.clone(), r.data_json.clone())
    } else {
        return view! {
            <div class="board-empty-tile">
                <p>"No result for this query yet. Open Resources, Test the query, then refresh."</p>
            </div>
        }
        .into_any();
    };

    if !ok {
        return view! {
            <div class="board-empty-tile">
                <p class="error-banner">{error.unwrap_or_else(|| "Query failed".into())}</p>
            </div>
        }
        .into_any();
    }

    match http_mode {
        HttpDisplayMode::Metric => {
            let (value, label, meta) = crate::app::dashboard::bind::project_bound_metric(&data_json, &bind);
            view! {
                <div class="board-metric">
                    <strong class="board-metric-value board-metric-number">{value}</strong>
                    <span class="board-metric-meta">{if meta.is_empty() { label } else { format!("{label} · {meta}") }}</span>
                </div>
            }
            .into_any()
        }
        HttpDisplayMode::List => {
            let items = crate::app::dashboard::bind::project_bound_list(&data_json, &bind, 12);
            if items.is_empty() {
                return view! { <div class="board-empty-tile"><p>"No rows"</p></div> }.into_any();
            }
            view! {
                <ul class="board-list">
                    {items.into_iter().map(|(title, subtitle, meta)| {
                        let meta_empty = subtitle.is_empty() && meta.is_empty();
                        let meta_line = match (subtitle.is_empty(), meta.is_empty()) {
                            (true, true) => String::new(),
                            (false, true) => subtitle,
                            (true, false) => meta,
                            (false, false) => format!("{subtitle} · {meta}"),
                        };
                        view! {
                            <li class="board-list-row">
                                <div class="board-list-grow">
                                    <strong>{title}</strong>
                                    <span class="board-list-meta" hidden=meta_empty>{meta_line}</span>
                                </div>
                            </li>
                        }
                    }).collect_view()}
                </ul>
            }
            .into_any()
        }
        HttpDisplayMode::Table => {
            let (headers, rows) = crate::app::dashboard::bind::project_bound_table(&data_json, &bind, 20);
            if headers.is_empty() {
                return view! { <div class="board-empty-tile"><p>"No columns"</p></div> }.into_any();
            }
            view! {
                <div class="board-table-wrap">
                    <table class="board-table">
                        <thead>
                            <tr>
                                {headers.into_iter().map(|h| view! { <th>{h}</th> }).collect_view()}
                            </tr>
                        </thead>
                        <tbody>
                            {rows.into_iter().map(|row| view! {
                                <tr>
                                    {row.into_iter().map(|cell| view! { <td>{cell}</td> }).collect_view()}
                                </tr>
                            }).collect_view()}
                        </tbody>
                    </table>
                </div>
            }
            .into_any()
        }
    }
}

fn parse_list_labels(data_json: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(data_json) else {
        return vec![data_json.to_owned()];
    };
    match value {
        serde_json::Value::Array(items) => items
            .into_iter()
            .map(|item| match item {
                serde_json::Value::String(s) => s,
                serde_json::Value::Object(map) => map
                    .get("name")
                    .or_else(|| map.get("title"))
                    .or_else(|| map.get("id"))
                    .map(|v| match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .unwrap_or_else(|| "{}".to_owned()),
                other => other.to_string(),
            })
            .collect(),
        serde_json::Value::String(s) => vec![s],
        other => vec![other.to_string()],
    }
}

fn relative_ms(ms: u64) -> String {
    let now_ms = current_unix_ms();
    if ms > now_ms {
        let delta = ms - now_ms;
        if delta < 60_000 {
            return "soon".to_owned();
        }
        if delta < 3_600_000 {
            return format!("in {}m", delta / 60_000);
        }
        return format!("in {}h", delta / 3_600_000);
    }
    let delta = now_ms.saturating_sub(ms);
    if delta < 60_000 {
        return "just now".to_owned();
    }
    if delta < 3_600_000 {
        return format!("{}m ago", delta / 60_000);
    }
    if delta < 86_400_000 {
        return format!("{}h ago", delta / 3_600_000);
    }
    format!("{}d ago", delta / 86_400_000)
}

fn current_unix_ms() -> u64 {
    #[cfg(feature = "hydrate")]
    {
        js_sys::Date::now() as u64
    }
    #[cfg(not(feature = "hydrate"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}
