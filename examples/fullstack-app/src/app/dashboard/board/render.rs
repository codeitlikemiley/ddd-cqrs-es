//! Board node / widget rendering.

#![allow(unused_imports)]

use super::layout::{
    commit_layout, find_col_span, find_node, remove_node, reorder_siblings, set_span_by_id,
};
use super::util::{current_unix_ms, event_target_value_board, parse_list_labels, relative_ms};
use crate::app::{DismissDashboardNotification, SaveDashboardLayout, UpdateDashboardNote};
use crate::contracts::{
    BoardContainerKind, BoardNode, DashboardLayout, DashboardNotification, DashboardSnapshot,
    DashboardWidgetKind, HttpDisplayMode, HttpQueryResult, QueryResult, QuerySummary, WidgetBind,
};
use crate::ui::classes::{
    BANNER_ERROR, BOARD_BIND_EDITOR, BOARD_BIND_FIELD, BOARD_BIND_FIELD_LABEL, BOARD_BIND_HINT,
    BOARD_BIND_ROW, BOARD_CHECKLIST, BOARD_CHECKLIST_ITEM, BOARD_CHECKLIST_ITEM_DONE,
    BOARD_CHECKLIST_LG, BOARD_CONTAINER_BODY, BOARD_CONTAINER_BODY_STACK, BOARD_CONTAINER_HEAD,
    BOARD_DRAG_HANDLE, BOARD_EMPTY_TILE, BOARD_FEED, BOARD_FEED_COPY, BOARD_FEED_DOT,
    BOARD_FEED_DOT_ERR, BOARD_FEED_DOT_OK, BOARD_FEED_ITEM, BOARD_INLINE_LINK,
    BOARD_INLINE_LINK_FLUSH, BOARD_INLINE_LINKS, BOARD_LIST, BOARD_LIST_GROW, BOARD_LIST_META,
    BOARD_LIST_ROW, BOARD_LIST_STRONG, BOARD_METRIC, BOARD_METRIC_META, BOARD_METRIC_NUMBER,
    BOARD_METRIC_VALUE, BOARD_NODE_SLOT, BOARD_NOTES, BOARD_NOTES_INPUT, BOARD_NOTIF,
    BOARD_NOTIF_BODY, BOARD_NOTIF_COPY, BOARD_NOTIF_DISMISS, BOARD_NOTIF_INFO, BOARD_NOTIF_LIST,
    BOARD_NOTIF_TIME, BOARD_NOTIF_TITLE, BOARD_NOTIF_WARN, BOARD_PILL, BOARD_PILL_LIVE,
    BOARD_PULSE, BOARD_SCORE_BAR, BOARD_SCORE_BAR_FILL, BOARD_SECURITY, BOARD_SECURITY_SCORE,
    BOARD_SECURITY_SCORE_LABEL, BOARD_SECURITY_SCORE_VALUE, BOARD_SPAN_CHIP,
    BOARD_SPAN_CHIP_ACTIVE, BOARD_SPAN_GROUP, BOARD_TABLE, BOARD_TABLE_TD, BOARD_TABLE_TH,
    BOARD_TABLE_WRAP, BOARD_TILE_BODY, BOARD_TILE_BODY_DIMMED, BOARD_TILE_CONTROLS,
    BOARD_TILE_HEAD, BOARD_TILE_HEAD_MAIN, BOARD_TILE_KICKER, BOARD_TILE_REMOVE, INPUT, MUTED,
    board_container_class, board_tile_class,
};
use leptos::prelude::*;
#[cfg(feature = "hydrate")]
use wasm_bindgen::JsCast;

pub(crate) fn render_node_list(
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
                <div class=BOARD_NODE_SLOT data-node-id=key>
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

pub(crate) fn render_node(
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
            let id_cls = id.clone();
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
                    class=move || {
                        let is_drop = drop_target.get().as_deref() == Some(id_cls.as_str())
                            && drag_id.get().as_deref() != Some(id_cls.as_str());
                        board_container_class(editing.get(), is_drop)
                    }
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
                    <header class=BOARD_CONTAINER_HEAD>
                        <span class=BOARD_TILE_KICKER>{kind_label}</span>
                        <Show when=move || editing.get()>
                            <div
                                class=BOARD_TILE_CONTROLS
                                draggable="false"
                                on:mousedown=move |e| e.stop_propagation()
                                on:dragstart=move |e| {
                                    e.prevent_default();
                                    e.stop_propagation();
                                }
                            >
                                {span_chips(id_chips.clone(), layout, save_layout)}
                                <button type="button" class=BOARD_TILE_REMOVE aria-label="Remove container" on:click={
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
                    <div class=if is_row { BOARD_CONTAINER_BODY } else { BOARD_CONTAINER_BODY_STACK }>
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
                                    <p class=MUTED>"Empty container — add tiles at the root level for now; nest by grouping with rows."</p>
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
                    class=move || {
                        let is_drop = drop_target.get().as_deref() == Some(id_cls.as_str())
                            && drag_id.get().as_deref() != Some(id_cls.as_str());
                        board_tile_class(editing.get(), is_drop)
                    }
                    data-span=span_attr
                    style=grid_style
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
                    <header class=BOARD_TILE_HEAD>
                        <div class=BOARD_TILE_HEAD_MAIN>
                            <Show when=move || editing.get()>
                                <span class=BOARD_DRAG_HANDLE aria-hidden="true">"⠿"</span>
                            </Show>
                            <p class=BOARD_TILE_KICKER>{kind_label}</p>
                        </div>
                        <Show when=move || editing.get()>
                            {
                                let id_chips = id_chips.clone();
                                let id_remove = id_remove.clone();
                                view! {
                                    <div
                                        class=BOARD_TILE_CONTROLS
                                        draggable="false"
                                        on:mousedown=move |e| e.stop_propagation()
                                        on:dragstart=move |e| {
                                            e.prevent_default();
                                            e.stop_propagation();
                                        }
                                    >
                                        {span_chips(id_chips, layout, save_layout)}
                                        <button type="button" class=BOARD_TILE_REMOVE aria-label="Remove" on:click=move |_| {
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
                    <div class=move || if editing.get() && !is_bound { BOARD_TILE_BODY_DIMMED } else { BOARD_TILE_BODY }>{body}</div>
                </article>
            }
            .into_any()
        }
    }
}

pub(crate) fn update_widget_bind(
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

pub(crate) fn bind_fields_editor(
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
            class=BOARD_BIND_EDITOR
            draggable="false"
            on:mousedown=move |e| e.stop_propagation()
            on:dragstart=move |e| {
                e.prevent_default();
                e.stop_propagation();
            }
        >
            <label class=BOARD_BIND_FIELD>
                <span class=BOARD_BIND_FIELD_LABEL>"Query"</span>
                <select
                    class=INPUT
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
            <label class=BOARD_BIND_FIELD>
                <span class=BOARD_BIND_FIELD_LABEL>"Display"</span>
                <select
                    class=INPUT
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
            <label class=BOARD_BIND_FIELD>
                <span class=BOARD_BIND_FIELD_LABEL>"Items path"</span>
                <input class=INPUT prop:value=items_path placeholder="e.g. data.items"
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
            <div class=BOARD_BIND_ROW>
                <label class=BOARD_BIND_FIELD>
                    <span class=BOARD_BIND_FIELD_LABEL>"Value / title path"</span>
                    <input class=INPUT prop:value=if matches!(mode, HttpDisplayMode::Metric) { value_path.clone() } else { title_path.clone() }
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
                <label class=BOARD_BIND_FIELD>
                    <span class=BOARD_BIND_FIELD_LABEL>"Label / subtitle path"</span>
                    <input class=INPUT prop:value=if matches!(mode, HttpDisplayMode::Metric) { label_path } else { subtitle_path }
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
            <label class=BOARD_BIND_FIELD>
                <span class=BOARD_BIND_FIELD_LABEL>"Meta path"</span>
                <input class=INPUT prop:value=meta_path placeholder="optional"
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
            <p class=BOARD_BIND_HINT>"Bind paths are dotted JSON paths relative to items path (or root). Table auto-detects columns when empty."</p>
        </div>
    }
    .into_any()
}

pub(crate) fn span_chips(
    id: String,
    layout: RwSignal<DashboardLayout>,
    save_layout: ServerAction<SaveDashboardLayout>,
) -> AnyView {
    let presets = [(3u8, "¼"), (4, "⅓"), (6, "½"), (12, "Full")];
    view! {
        <div class=BOARD_SPAN_GROUP role="group" aria-label="Width">
            {presets.into_iter().map(|(size, label)| {
                let id = id.clone();
                let id_active = id.clone();
                view! {
                    <button
                        type="button"
                        class=move || {
                            if find_col_span(&layout.get().nodes, &id_active).unwrap_or(0) == size {
                                BOARD_SPAN_CHIP_ACTIVE
                            } else {
                                BOARD_SPAN_CHIP
                            }
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

pub(crate) fn render_widget_body(
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
        return view! { <p class=MUTED>"Loading…"</p> }.into_any();
    };

    match kind {
        DashboardWidgetKind::MetricSession => view! {
            <div class=BOARD_METRIC>
                <strong class=BOARD_METRIC_VALUE>
                    <span class=BOARD_PULSE aria-hidden="true"></span>"Verified"
                </strong>
                <span class=BOARD_METRIC_META>{data.assurance.to_uppercase()}</span>
            </div>
        }
        .into_any(),
        DashboardWidgetKind::MetricDevices => view! {
            <div class=BOARD_METRIC>
                <strong class=BOARD_METRIC_NUMBER>{data.active_session_count.to_string()}</strong>
                <span class=BOARD_METRIC_META>"signed-in sessions"</span>
            </div>
        }
        .into_any(),
        DashboardWidgetKind::MetricOrgs => view! {
            <div class=BOARD_METRIC>
                <strong class=BOARD_METRIC_NUMBER>{data.organization_count.to_string()}</strong>
                <span class=BOARD_METRIC_META>"workspaces"</span>
            </div>
        }
        .into_any(),
        DashboardWidgetKind::MetricSecurity => view! {
            <div class=BOARD_METRIC>
                <strong class=BOARD_METRIC_NUMBER>{format!("{}%", data.security_score)}</strong>
                <span class=BOARD_METRIC_META>{if data.totp_enrolled { "MFA on" } else { "MFA off" }}</span>
                <div class=BOARD_SCORE_BAR aria-hidden="true"><span class=BOARD_SCORE_BAR_FILL style=format!("width:{}%", data.security_score)></span></div>
            </div>
        }
        .into_any(),
        DashboardWidgetKind::Activity => {
            if data.activity.is_empty() {
                view! {
                    <div class=BOARD_EMPTY_TILE>
                        <p>{if data.has_tenant { "No audit events yet." } else { "Select an organization to stream activity." }}</p>
                    </div>
                }
                .into_any()
            } else {
                view! {
                    <ul class=BOARD_FEED>
                        {data.activity.into_iter().take(8).map(|event| {
                            let outcome = event.outcome.clone();
                            let dot = match outcome.as_str() {
                                "success" | "allowed" | "ok" => BOARD_FEED_DOT_OK,
                                "denied" | "failed" | "error" => BOARD_FEED_DOT_ERR,
                                _ => BOARD_FEED_DOT,
                            };
                            view! {
                                <li class=BOARD_FEED_ITEM>
                                    <span class=dot></span>
                                    <div class=BOARD_FEED_COPY>
                                        <strong class=BOARD_LIST_STRONG>{event.action}</strong>
                                        <span class=BOARD_LIST_META>{format!("{} · {}", event.outcome, relative_ms(event.recorded_at_ms))}</span>
                                    </div>
                                </li>
                            }
                        }).collect_view()}
                    </ul>
                }
                .into_any()
            }
        }
        DashboardWidgetKind::Notifications => {
            if notifications.is_empty() {
                view! { <div class=BOARD_EMPTY_TILE><p>"You're caught up."</p></div> }.into_any()
            } else {
                view! {
                    <ul class=BOARD_NOTIF_LIST>
                        {notifications.into_iter().map(|item| {
                            let id = item.id.clone();
                            let notif_class = match item.level.as_str() {
                                "warn" => BOARD_NOTIF_WARN,
                                "info" => BOARD_NOTIF_INFO,
                                _ => BOARD_NOTIF,
                            };
                            view! {
                                <li class=notif_class>
                                    <div class=BOARD_NOTIF_COPY>
                                        <strong class=BOARD_NOTIF_TITLE>{item.title}</strong>
                                        <p class=BOARD_NOTIF_BODY>{item.body}</p>
                                        <span class=BOARD_NOTIF_TIME>{relative_ms(item.created_at_ms)}</span>
                                    </div>
                                    <button type="button" class=BOARD_NOTIF_DISMISS on:click=move |_| {
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
            <ul class=BOARD_LIST>
                {data.sessions.into_iter().take(6).map(|session| view! {
                    <li class=BOARD_LIST_ROW>
                        <div class=BOARD_LIST_GROW>
                            <strong class=BOARD_LIST_STRONG>{if session.current { "This browser" } else { "Other session" }}</strong>
                            <span class=BOARD_LIST_META>{format!("{} · {}", session.assurance.to_uppercase(), relative_ms(session.expires_at_ms))}</span>
                        </div>
                        <span class=if session.current { BOARD_PILL_LIVE } else { BOARD_PILL }>
                            {if session.current { "Live" } else { "Active" }}
                        </span>
                    </li>
                }).collect_view()}
            </ul>
            <a class=BOARD_INLINE_LINK href="/account/sessions">"Manage sessions"</a>
        }
        .into_any(),
        DashboardWidgetKind::Organizations => {
            if data.organizations.is_empty() {
                view! { <div class=BOARD_EMPTY_TILE><p>"No workspaces yet."</p><a class=BOARD_INLINE_LINK href="/organizations">"Create one"</a></div> }.into_any()
            } else {
                let active = data.tenant_label.clone();
                view! {
                    <ul class=BOARD_LIST>
                        {data.organizations.into_iter().take(6).map(|org| {
                            let is_active = active.as_ref().is_some_and(|t| t == &org.organization_id);
                            view! {
                                <li class=BOARD_LIST_ROW>
                                    <div class=BOARD_LIST_GROW>
                                        <strong class=BOARD_LIST_STRONG>{org.name}</strong>
                                        <span class=BOARD_LIST_META>{org.current_user_role}</span>
                                    </div>
                                    <span class=if is_active { BOARD_PILL_LIVE } else { BOARD_PILL }>
                                        {if is_active { "Active" } else { "Joined" }}
                                    </span>
                                </li>
                            }
                        }).collect_view()}
                    </ul>
                    <a class=BOARD_INLINE_LINK href="/organizations">"All organizations"</a>
                }
                .into_any()
            }
        }
        DashboardWidgetKind::SecurityPosture => view! {
            <div class=BOARD_SECURITY>
                <div class=BOARD_SECURITY_SCORE>
                    <strong class=BOARD_SECURITY_SCORE_VALUE>{format!("{}%", data.security_score)}</strong>
                    <span class=BOARD_SECURITY_SCORE_LABEL>"posture"</span>
                </div>
                <ul class=BOARD_CHECKLIST>
                    <li class=if data.totp_enrolled { BOARD_CHECKLIST_ITEM_DONE } else { BOARD_CHECKLIST_ITEM }>
                        {if data.totp_enrolled { "Authenticator enrolled" } else { "Enroll authenticator" }}
                    </li>
                    <li class={if data.recovery_codes_remaining > 0 { BOARD_CHECKLIST_ITEM_DONE } else { BOARD_CHECKLIST_ITEM }}>
                        {format!("{} recovery codes left", data.recovery_codes_remaining)}
                    </li>
                    <li class={if data.active_session_count <= 3 { BOARD_CHECKLIST_ITEM_DONE } else { BOARD_CHECKLIST_ITEM }}>
                        {format!("{} active sessions", data.active_session_count)}
                    </li>
                </ul>
                <div class=BOARD_INLINE_LINKS>
                    <a class=BOARD_INLINE_LINK_FLUSH href="/account/mfa">"MFA"</a>
                    <a class=BOARD_INLINE_LINK_FLUSH href="/account/passkeys">"Passkeys"</a>
                    <a class=BOARD_INLINE_LINK_FLUSH href="/account/sessions">"Sessions"</a>
                </div>
            </div>
        }
        .into_any(),
        DashboardWidgetKind::Notes => {
            let widget_id_save = widget_id.clone();
            view! {
                <div class=BOARD_NOTES>
                    <textarea class=BOARD_NOTES_INPUT rows="5" maxlength="2000"
                        placeholder="Scratch pad — only you can see this."
                        prop:value=note_text
                        on:blur=move |event| {
                            note_action.dispatch(UpdateDashboardNote {
                                widget_id: widget_id_save.clone(),
                                text: event_target_value(&event),
                            });
                        }
                    />
                    <p class=MUTED>"Saves when you leave the field."</p>
                </div>
            }
            .into_any()
        }
        DashboardWidgetKind::Checklist => view! {
            <ul class=BOARD_CHECKLIST_LG>
                <li class=if data.email.is_some() { BOARD_CHECKLIST_ITEM_DONE } else { BOARD_CHECKLIST_ITEM }>
                    <a href="/account/profile">"Complete profile"</a>
                </li>
                <li class={if data.organization_count > 0 { BOARD_CHECKLIST_ITEM_DONE } else { BOARD_CHECKLIST_ITEM }}>
                    <a href="/organizations">"Create or join an organization"</a>
                </li>
                <li class=if data.has_tenant { BOARD_CHECKLIST_ITEM_DONE } else { BOARD_CHECKLIST_ITEM }>
                    <a href="/organizations">"Select an active tenant"</a>
                </li>
                <li class=if data.totp_enrolled { BOARD_CHECKLIST_ITEM_DONE } else { BOARD_CHECKLIST_ITEM }>
                    <a href="/account/mfa">"Turn on multi-factor auth"</a>
                </li>
            </ul>
        }
        .into_any(),
        DashboardWidgetKind::HttpPanel
        | DashboardWidgetKind::BoundMetric
        | DashboardWidgetKind::BoundList
        | DashboardWidgetKind::BoundTable => {
            view! { <p class=MUTED>"Query-bound widget"</p> }.into_any()
        }
    }
}

pub(crate) fn render_bound_widget(
    source_id: Option<String>,
    bind: WidgetBind,
    http_mode: HttpDisplayMode,
    http_results: &[HttpQueryResult],
    query_results: &[QueryResult],
) -> AnyView {
    let Some(qid) = source_id.filter(|s| !s.is_empty()) else {
        return view! {
            <div class=BOARD_EMPTY_TILE>
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
            <div class=BOARD_EMPTY_TILE>
                <p>"No result for this query yet. Open Resources, Test the query, then refresh."</p>
            </div>
        }
        .into_any();
    };

    if !ok {
        return view! {
            <div class=BOARD_EMPTY_TILE>
                <p class=BANNER_ERROR>{error.unwrap_or_else(|| "Query failed".into())}</p>
            </div>
        }
        .into_any();
    }

    match http_mode {
        HttpDisplayMode::Metric => {
            let (value, label, meta) =
                crate::app::dashboard::bind::project_bound_metric(&data_json, &bind);
            view! {
                <div class=BOARD_METRIC>
                    <strong class=BOARD_METRIC_NUMBER>{value}</strong>
                    <span class=BOARD_METRIC_META>{if meta.is_empty() { label } else { format!("{label} · {meta}") }}</span>
                </div>
            }
            .into_any()
        }
        HttpDisplayMode::List => {
            let items = crate::app::dashboard::bind::project_bound_list(&data_json, &bind, 12);
            if items.is_empty() {
                return view! { <div class=BOARD_EMPTY_TILE><p>"No rows"</p></div> }.into_any();
            }
            view! {
                <ul class=BOARD_LIST>
                    {items.into_iter().map(|(title, subtitle, meta)| {
                        let meta_empty = subtitle.is_empty() && meta.is_empty();
                        let meta_line = match (subtitle.is_empty(), meta.is_empty()) {
                            (true, true) => String::new(),
                            (false, true) => subtitle,
                            (true, false) => meta,
                            (false, false) => format!("{subtitle} · {meta}"),
                        };
                        view! {
                            <li class=BOARD_LIST_ROW>
                                <div class=BOARD_LIST_GROW>
                                    <strong class=BOARD_LIST_STRONG>{title}</strong>
                                    <span class=BOARD_LIST_META hidden=meta_empty>{meta_line}</span>
                                </div>
                            </li>
                        }
                    }).collect_view()}
                </ul>
            }
            .into_any()
        }
        HttpDisplayMode::Table => {
            let (headers, rows) =
                crate::app::dashboard::bind::project_bound_table(&data_json, &bind, 20);
            if headers.is_empty() {
                return view! { <div class=BOARD_EMPTY_TILE><p>"No columns"</p></div> }
                    .into_any();
            }
            view! {
                <div class=BOARD_TABLE_WRAP>
                    <table class=BOARD_TABLE>
                        <thead>
                            <tr>
                                {headers.into_iter().map(|h| view! { <th class=BOARD_TABLE_TH>{h}</th> }).collect_view()}
                            </tr>
                        </thead>
                        <tbody>
                            {rows.into_iter().map(|row| view! {
                                <tr>
                                    {row.into_iter().map(|cell| view! { <td class=BOARD_TABLE_TD>{cell}</td> }).collect_view()}
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
