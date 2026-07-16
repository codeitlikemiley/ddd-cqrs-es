//! Workspace settings — Audit activity (humanized table, filters, details).

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use super::shared::{
    format_relative_time_ms, format_settings_datetime_ms, settings_page_stub,
    slug_from_settings_pathname,
};
use crate::app::helpers::{current_browser_pathname, server_error_text};
use crate::app::{
    browser_load, get_workspace_settings_context, list_workspace_audit, list_workspace_members,
    list_workspace_roles,
};
use crate::contracts::AuditEventSummary;
use crate::ui::classes::{
    BANNER_ERROR, BTN_SECONDARY, INPUT, MUTED, RESULT_LINE, SR_ONLY, VAULT_MODAL_BACKDROP,
    VAULT_MODAL_BODY, VAULT_MODAL_CLOSE, VAULT_MODAL_HEAD, VAULT_MODAL_HEAD_P,
    VAULT_MODAL_HEAD_TITLE, WS_AUDIT_COL_ACTION, WS_AUDIT_COL_ACTOR, WS_AUDIT_COL_DETAILS,
    WS_AUDIT_COL_OUTCOME, WS_AUDIT_COL_TARGET, WS_AUDIT_COL_WHEN, WS_AUDIT_DETAIL_DD,
    WS_AUDIT_DETAIL_DT, WS_AUDIT_DETAIL_LIST, WS_AUDIT_DETAIL_MODAL, WS_AUDIT_DETAIL_ROW,
    WS_AUDIT_ELLIPSIS, WS_AUDIT_EYE, WS_AUDIT_FILTER, WS_AUDIT_FILTER_ACTOR, WS_AUDIT_FILTER_LABEL,
    WS_AUDIT_FILTERS, WS_AUDIT_FOOTER, WS_AUDIT_HINT, WS_AUDIT_ICON_BUTTON, WS_AUDIT_JSON,
    WS_AUDIT_METADATA_H3, WS_AUDIT_METADATA_P, WS_AUDIT_TOOLBAR, WS_AUDIT_WHEN, WS_EMPTY,
    WS_MODAL_ACTIONS, WS_MONO, WS_TABLE_AUDIT, WS_TABLE_WRAP, WS_TD, WS_TH, WS_THEAD, WS_TR,
    ws_outcome_icon, ws_status_pill, with_extra,
};
use crate::ui::{
    ComboboxOption, FilterCombobox, SettingsPageSkeleton, SettingsSkeletonVariant,
};
use leptos::prelude::*;
#[cfg(feature = "hydrate")]
use leptos::task::spawn_local;
use std::collections::{BTreeMap, BTreeSet};

const PAGE_LIMIT: u32 = 50;

#[component]
pub fn WorkspaceSettingsAuditPage() -> impl IntoView {
    settings_page_stub(
        "Audit log",
        "Security and administration activity for this workspace.",
        "Requires audit.view. Cursor pages load newest sequences after the last row.",
        view! { <WorkspaceSettingsAuditBody /> },
    )
}

/// Island: humanized audit table with client filters, load-more, and details drawer.
#[island]
pub fn WorkspaceSettingsAuditBody() -> impl IntoView {
    let slug = Memo::new(move |_| slug_from_settings_pathname(&current_browser_pathname()));

    let initial = browser_load({
        move || {
            let slug = slug_from_settings_pathname(&current_browser_pathname());
            list_workspace_audit(slug, None, Some(PAGE_LIMIT))
        }
    });
    let members = browser_load({
        move || {
            let slug = slug_from_settings_pathname(&current_browser_pathname());
            list_workspace_members(slug)
        }
    });
    let context = browser_load({
        move || {
            let slug = slug_from_settings_pathname(&current_browser_pathname());
            get_workspace_settings_context(slug)
        }
    });
    let roles = browser_load({
        move || {
            let slug = slug_from_settings_pathname(&current_browser_pathname());
            list_workspace_roles(slug)
        }
    });

    let (events, set_events) = signal(Vec::<AuditEventSummary>::new());
    let (next_cursor, set_next_cursor) = signal(0u64);
    let (has_more, set_has_more) = signal(false);
    let (list_ready, set_list_ready) = signal(false);
    let (loading_more, set_loading_more) = signal(false);
    let (load_error, set_load_error) = signal(None::<String>);
    let (actor_emails, set_actor_emails) = signal(BTreeMap::<String, String>::new());
    let (workspace_name, set_workspace_name) = signal(String::new());
    let (workspace_slug, set_workspace_slug) = signal(String::new());
    let (role_names, set_role_names) = signal(BTreeMap::<String, String>::new());

    let filter_action = RwSignal::new(String::new());
    let filter_outcome = RwSignal::new(String::new());
    let (filter_actor, set_filter_actor) = signal(String::new());
    let (selected, set_selected) = signal(None::<AuditEventSummary>);

    Effect::new(move |_| {
        if list_ready.get() {
            return;
        }
        match initial.get() {
            Some(Ok(page)) => {
                let more = page_has_more(&page.events, page.next_cursor, PAGE_LIMIT);
                set_events.set(page.events);
                set_next_cursor.set(page.next_cursor);
                set_has_more.set(more);
                set_list_ready.set(true);
                set_load_error.set(None);
            }
            Some(Err(error)) => {
                set_load_error.set(Some(server_error_text(error)));
                set_list_ready.set(true);
            }
            None => {}
        }
    });

    Effect::new(move |_| {
        if let Some(Ok(list)) = members.get() {
            let mut map = BTreeMap::new();
            for membership in list.memberships {
                let email = membership.primary_email.trim();
                if !email.is_empty() {
                    map.insert(membership.user_id, email.to_owned());
                }
            }
            set_actor_emails.set(map);
        }
    });

    Effect::new(move |_| {
        if let Some(Ok(ctx)) = context.get() {
            set_workspace_name.set(ctx.organization.name);
            set_workspace_slug.set(ctx.organization.slug);
        }
    });

    Effect::new(move |_| {
        if let Some(Ok(list)) = roles.get() {
            let mut map = BTreeMap::new();
            for role in list.roles {
                let name = if role.name.trim().is_empty() {
                    role.role_id.clone()
                } else {
                    role.name
                };
                map.insert(role.role_id, name);
            }
            set_role_names.set(map);
        }
    });

    let filtered = Memo::new(move |_| {
        let action = filter_action.get();
        let outcome = filter_outcome.get();
        let actor_q = filter_actor.get().trim().to_ascii_lowercase();
        let emails = actor_emails.get();
        events
            .get()
            .into_iter()
            .filter(|event| {
                if !action.is_empty() && event.action != action {
                    return false;
                }
                if !outcome.is_empty() && event.outcome != outcome {
                    return false;
                }
                if !actor_q.is_empty() {
                    let email = emails
                        .get(&event.actor_user_id)
                        .map(String::as_str)
                        .unwrap_or("");
                    let hay = format!(
                        "{} {}",
                        event.actor_user_id.to_ascii_lowercase(),
                        email.to_ascii_lowercase()
                    );
                    if !hay.contains(&actor_q) {
                        return false;
                    }
                }
                true
            })
            .collect::<Vec<_>>()
    });

    let action_options = Memo::new(move |_| {
        let mut set = BTreeSet::new();
        for key in KNOWN_AUDIT_ACTIONS {
            set.insert((*key).to_owned());
        }
        for event in events.get() {
            if !event.action.is_empty() {
                set.insert(event.action);
            }
        }
        set.into_iter()
            .map(|value| ComboboxOption {
                label: humanize_audit_action(&value),
                value,
            })
            .collect::<Vec<_>>()
    });

    let outcome_options = Memo::new(move |_| {
        let mut set = BTreeSet::new();
        for key in KNOWN_AUDIT_OUTCOMES {
            set.insert((*key).to_owned());
        }
        for event in events.get() {
            if !event.outcome.is_empty() {
                set.insert(event.outcome);
            }
        }
        set.into_iter()
            .map(|value| ComboboxOption {
                label: humanize_audit_outcome(&value),
                value,
            })
            .collect::<Vec<_>>()
    });

    let action_options_signal = Signal::derive(move || action_options.get());
    let outcome_options_signal = Signal::derive(move || outcome_options.get());
    let busy = Memo::new(move |_| loading_more.get());

    view! {
        <Show when=move || !list_ready.get() && load_error.get().is_none()>
            <SettingsPageSkeleton
                label="Loading audit events"
                variant=SettingsSkeletonVariant::Table
                show_header=false
            />
        </Show>

        <Show when=move || load_error.get().is_some()>
            <p class=BANNER_ERROR>{move || load_error.get().unwrap_or_default()}</p>
        </Show>

        <Show when=move || list_ready.get()>
            <div class=WS_AUDIT_TOOLBAR data-settings-wide="">
                <div class=WS_AUDIT_FILTERS>
                    <FilterCombobox
                        label="Action"
                        all_label="All actions"
                        options=action_options_signal
                        value=filter_action
                        disabled=Signal::derive(move || busy.get())
                    />
                    <FilterCombobox
                        label="Outcome"
                        all_label="All outcomes"
                        options=outcome_options_signal
                        value=filter_outcome
                        disabled=Signal::derive(move || busy.get())
                    />
                    <label class=with_extra(WS_AUDIT_FILTER, Some(WS_AUDIT_FILTER_ACTOR))>
                        <span class=WS_AUDIT_FILTER_LABEL>"Actor"</span>
                        <input
                            class=INPUT
                            type="search"
                            placeholder="Email"
                            prop:value=move || filter_actor.get()
                            disabled=move || busy.get()
                            on:input=move |event| {
                                set_filter_actor.set(event_target_value(&event));
                            }
                        />
                    </label>
                </div>
            </div>

            <p class=with_extra(MUTED, Some(WS_AUDIT_HINT))>
                "Filters apply to loaded events. Use Load more to fetch older pages."
            </p>

            {move || {
                let rows = filtered.get();
                let total_loaded = events.get().len();
                if total_loaded == 0 {
                    return view! {
                        <div class=WS_EMPTY role="status">
                            <p>"No audit events yet."</p>
                            <p class=MUTED>
                                "Workspace administration and security actions will appear here."
                            </p>
                        </div>
                    }
                    .into_any();
                }
                if rows.is_empty() {
                    return view! {
                        <div class=WS_EMPTY role="status">
                            <p>"No events match the current filters."</p>
                            <p class=MUTED>
                                {format!(
                                    "{total_loaded} event(s) loaded. Clear filters or load more."
                                )}
                            </p>
                        </div>
                    }
                    .into_any();
                }

                let emails = actor_emails.get();
                let org_name = workspace_name.get();
                let org_slug = workspace_slug.get();
                let roles_map = role_names.get();
                view! {
                    <div class=WS_TABLE_WRAP>
                        <table class=WS_TABLE_AUDIT>
                            <thead class=WS_THEAD>
                                <tr>
                                    <th scope="col" class=format!("{} {}", WS_TH, WS_AUDIT_COL_WHEN)>"When"</th>
                                    <th scope="col" class=format!("{} {}", WS_TH, WS_AUDIT_COL_ACTOR)>"Actor"</th>
                                    <th scope="col" class=format!("{} {}", WS_TH, WS_AUDIT_COL_ACTION)>"Action"</th>
                                    <th scope="col" class=format!("{} {}", WS_TH, WS_AUDIT_COL_TARGET)>"Target"</th>
                                    <th scope="col" class=format!("{} {}", WS_TH, WS_AUDIT_COL_OUTCOME)>"Outcome"</th>
                                    <th scope="col" class=format!("{} {}", WS_TH, WS_AUDIT_COL_DETAILS)>
                                        <span class=SR_ONLY>"Details"</span>
                                    </th>
                                </tr>
                            </thead>
                            <tbody>
                                {rows
                                    .into_iter()
                                    .map(|event| {
                                        audit_row(
                                            event,
                                            emails.clone(),
                                            org_name.clone(),
                                            org_slug.clone(),
                                            roles_map.clone(),
                                            set_selected,
                                        )
                                    })
                                    .collect_view()}
                            </tbody>
                        </table>
                    </div>
                }
                .into_any()
            }}

            <div class=WS_AUDIT_FOOTER>
                <p class=MUTED>
                    {move || {
                        format!(
                            "Showing {} of {} loaded",
                            filtered.get().len(),
                            events.get().len()
                        )
                    }}
                </p>
                <Show when=move || has_more.get()>
                    <button
                        type="button"
                        class=BTN_SECONDARY
                        disabled=move || busy.get() || slug.get().is_empty()
                        on:click=move |_| {
                            let slug_value = slug.get_untracked();
                            if slug_value.is_empty() {
                                return;
                            }
                            let after = next_cursor.get_untracked();
                            set_loading_more.set(true);
                            set_load_error.set(None);
                            fetch_audit_page(
                                slug_value,
                                Some(after.to_string()),
                                set_events,
                                set_next_cursor,
                                set_has_more,
                                set_loading_more,
                                set_load_error,
                                false,
                            );
                        }
                    >
                        {move || {
                            if loading_more.get() {
                                "Loading…"
                            } else {
                                "Load more"
                            }
                        }}
                    </button>
                </Show>
            </div>
        </Show>

        <Show when=move || selected.get().is_some()>
            <div
                class=VAULT_MODAL_BACKDROP
                role="presentation"
                on:click=move |_| set_selected.set(None)
            >
                <div
                    class=WS_AUDIT_DETAIL_MODAL
                    role="dialog"
                    aria-modal="true"
                    aria-labelledby="workspace-audit-detail-title"
                    on:click=move |e| e.stop_propagation()
                >
                    <header class=VAULT_MODAL_HEAD>
                        <div>
                            <h2 id="workspace-audit-detail-title" class=VAULT_MODAL_HEAD_TITLE>"Event details"</h2>
                            <p class=VAULT_MODAL_HEAD_P>
                                {move || {
                                    selected
                                        .get()
                                        .map(|event| humanize_audit_action(&event.action))
                                        .unwrap_or_default()
                                }}
                            </p>
                        </div>
                        <button
                            type="button"
                            class=VAULT_MODAL_CLOSE
                            on:click=move |_| set_selected.set(None)
                        >
                            "Close"
                        </button>
                    </header>
                    <div class=VAULT_MODAL_BODY>
                        {move || {
                            let Some(event) = selected.get() else {
                                return view! { <></> }.into_any();
                            };
                            let emails = actor_emails.get();
                            let actor = actor_display(&event.actor_user_id, &emails);
                            let when = format_settings_datetime_ms(event.recorded_at_ms);
                            let relative = format_relative_time_ms(event.recorded_at_ms);
                            let org = event
                                .organization_id
                                .clone()
                                .filter(|v| !v.trim().is_empty())
                                .unwrap_or_else(|| "—".to_owned());
                            let target_label = resolve_target_label(
                                &event.target_type,
                                &event.target_id,
                                &workspace_name.get(),
                                &workspace_slug.get(),
                                &emails,
                                &role_names.get(),
                            );
                            let metadata_json = safe_event_metadata_json(&event);
                            view! {
                                <dl class=WS_AUDIT_DETAIL_LIST>
                                    <div class=WS_AUDIT_DETAIL_ROW>
                                        <dt class=WS_AUDIT_DETAIL_DT>"When"</dt>
                                        <dd class=WS_AUDIT_DETAIL_DD>
                                            <span>{when}</span>
                                            <small class=MUTED>{relative}</small>
                                        </dd>
                                    </div>
                                    <div class=WS_AUDIT_DETAIL_ROW>
                                        <dt class=WS_AUDIT_DETAIL_DT>"Sequence"</dt>
                                        <dd class=format!("{} {}", WS_AUDIT_DETAIL_DD, WS_MONO)>{event.sequence.to_string()}</dd>
                                    </div>
                                    <div class=WS_AUDIT_DETAIL_ROW>
                                        <dt class=WS_AUDIT_DETAIL_DT>"Actor"</dt>
                                        <dd class=WS_AUDIT_DETAIL_DD>
                                            <span>{actor}</span>
                                            <small class=with_extra(MUTED, Some(WS_MONO))>
                                                {event.actor_user_id.clone()}
                                            </small>
                                        </dd>
                                    </div>
                                    <div class=WS_AUDIT_DETAIL_ROW>
                                        <dt class=WS_AUDIT_DETAIL_DT>"Action"</dt>
                                        <dd class=WS_AUDIT_DETAIL_DD>
                                            <strong>{humanize_audit_action(&event.action)}</strong>
                                            <small class=with_extra(MUTED, Some(WS_MONO))>
                                                {event.action.clone()}
                                            </small>
                                        </dd>
                                    </div>
                                    <div class=WS_AUDIT_DETAIL_ROW>
                                        <dt class=WS_AUDIT_DETAIL_DT>"Target"</dt>
                                        <dd class=WS_AUDIT_DETAIL_DD>
                                            <span>{target_label}</span>
                                            <small class=MUTED>
                                                {format!(
                                                    "{} · {}",
                                                    event.target_type,
                                                    if event.target_id.is_empty() {
                                                        "—".to_owned()
                                                    } else {
                                                        event.target_id.clone()
                                                    }
                                                )}
                                            </small>
                                        </dd>
                                    </div>
                                    <div class=WS_AUDIT_DETAIL_ROW>
                                        <dt class=WS_AUDIT_DETAIL_DT>"Outcome"</dt>
                                        <dd class=WS_AUDIT_DETAIL_DD>
                                            <span class=ws_status_pill(&event.outcome)>
                                                {humanize_audit_outcome(&event.outcome)}
                                            </span>
                                        </dd>
                                    </div>
                                    <div class=WS_AUDIT_DETAIL_ROW>
                                        <dt class=WS_AUDIT_DETAIL_DT>"Organization id"</dt>
                                        <dd class=format!("{} {}", WS_AUDIT_DETAIL_DD, WS_MONO)>{org}</dd>
                                    </div>
                                </dl>
                                <div>
                                    <h3 class=WS_AUDIT_METADATA_H3>"Event payload"</h3>
                                    <p class=with_extra(MUTED, Some(WS_AUDIT_METADATA_P))>
                                        "Safe JSON of fields returned for this event (no secrets)."
                                    </p>
                                    <pre class=WS_AUDIT_JSON>{metadata_json}</pre>
                                </div>
                                <div class=WS_MODAL_ACTIONS>
                                    <button
                                        type="button"
                                        class=BTN_SECONDARY
                                        on:click=move |_| set_selected.set(None)
                                    >
                                        "Close"
                                    </button>
                                </div>
                            }
                            .into_any()
                        }}
                    </div>
                </div>
            </div>
        </Show>
    }
}

fn audit_row(
    event: AuditEventSummary,
    emails: BTreeMap<String, String>,
    workspace_name: String,
    workspace_slug: String,
    role_names: BTreeMap<String, String>,
    set_selected: WriteSignal<Option<AuditEventSummary>>,
) -> impl IntoView {
    let when_relative = format_relative_time_ms(event.recorded_at_ms);
    let when_absolute = format_settings_datetime_ms(event.recorded_at_ms);
    let when_iso = if event.recorded_at_ms == 0 {
        String::new()
    } else {
        // Approximate ISO for datetime attr; hydrate locale stays in title.
        format!("{}", event.recorded_at_ms)
    };
    let actor = actor_display(&event.actor_user_id, &emails);
    let actor_title = actor.clone();
    let action_label = humanize_audit_action(&event.action);
    let action_title = action_label.clone();
    let target = resolve_target_label(
        &event.target_type,
        &event.target_id,
        &workspace_name,
        &workspace_slug,
        &emails,
        &role_names,
    );
    let target_title = target.clone();
    let outcome = event.outcome.clone();
    let outcome_label = humanize_audit_outcome(&outcome);
    let outcome_title = outcome_label.clone();
    let outcome_icon = outcome_icon_glyph(&outcome);
    let event_for_click = event.clone();

    view! {
        <tr class=WS_TR>
            <td data-label="When" class=format!("{} {}", WS_TD, WS_AUDIT_COL_WHEN)>
                <time
                    class=WS_AUDIT_WHEN
                    datetime=when_iso
                    title=when_absolute
                >
                    {when_relative}
                </time>
            </td>
            <td data-label="Actor" class=format!("{} {}", WS_TD, WS_AUDIT_COL_ACTOR)>
                <span class=WS_AUDIT_ELLIPSIS title=actor_title>
                    {actor}
                </span>
            </td>
            <td data-label="Action" class=format!("{} {}", WS_TD, WS_AUDIT_COL_ACTION)>
                <span class=WS_AUDIT_ELLIPSIS title=action_title>
                    {action_label}
                </span>
            </td>
            <td data-label="Target" class=format!("{} {}", WS_TD, WS_AUDIT_COL_TARGET)>
                <span class=WS_AUDIT_ELLIPSIS title=target_title>
                    {target}
                </span>
            </td>
            <td data-label="Outcome" class=format!("{} {}", WS_TD, WS_AUDIT_COL_OUTCOME)>
                <span
                    class=ws_outcome_icon(&outcome_class(&outcome))
                    title=outcome_title
                    aria-label=outcome_label
                    role="img"
                >
                    {outcome_icon}
                </span>
            </td>
            <td data-label="Details" class=format!("{} {}", WS_TD, WS_AUDIT_COL_DETAILS)>
                <button
                    type="button"
                    class=WS_AUDIT_ICON_BUTTON
                    aria-label="View event details"
                    title="View details"
                    on:click=move |_| set_selected.set(Some(event_for_click.clone()))
                >
                    <svg
                        class=WS_AUDIT_EYE
                        viewBox="0 0 24 24"
                        width="16"
                        height="16"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.75"
                        aria-hidden="true"
                    >
                        <path
                            d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7-10-7-10-7z"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                        ></path>
                        <circle cx="12" cy="12" r="3"></circle>
                    </svg>
                </button>
            </td>
        </tr>
    }
}

fn fetch_audit_page(
    slug: String,
    after: Option<String>,
    set_events: WriteSignal<Vec<AuditEventSummary>>,
    set_next_cursor: WriteSignal<u64>,
    set_has_more: WriteSignal<bool>,
    set_busy: WriteSignal<bool>,
    set_load_error: WriteSignal<Option<String>>,
    replace: bool,
) {
    #[cfg(feature = "hydrate")]
    {
        spawn_local(async move {
            match list_workspace_audit(slug, after, Some(PAGE_LIMIT)).await {
                Ok(page) => {
                    let more = page_has_more(&page.events, page.next_cursor, PAGE_LIMIT);
                    if replace {
                        set_events.set(page.events);
                    } else {
                        set_events.update(|list| {
                            for event in page.events {
                                if !list
                                    .iter()
                                    .any(|existing| existing.sequence == event.sequence)
                                {
                                    list.push(event);
                                }
                            }
                        });
                    }
                    set_next_cursor.set(page.next_cursor);
                    set_has_more.set(more);
                    set_load_error.set(None);
                }
                Err(error) => {
                    set_load_error.set(Some(server_error_text(error)));
                }
            }
            set_busy.set(false);
        });
    }
    #[cfg(not(feature = "hydrate"))]
    {
        let _ = (
            slug,
            after,
            set_events,
            set_next_cursor,
            set_has_more,
            set_busy,
            set_load_error,
            replace,
        );
    }
}

fn page_has_more(events: &[AuditEventSummary], next_cursor: u64, limit: u32) -> bool {
    !events.is_empty() && events.len() as u32 >= limit && next_cursor > 0
}

fn actor_display(actor_user_id: &str, emails: &BTreeMap<String, String>) -> String {
    if let Some(email) = emails.get(actor_user_id) {
        if !email.trim().is_empty() {
            return email.clone();
        }
    }
    if actor_user_id == "system" || actor_user_id.is_empty() {
        return "System".to_owned();
    }
    // Prefer a soft fallback over raw UUID noise in the table.
    "Unknown member".to_owned()
}

fn resolve_target_label(
    target_type: &str,
    target_id: &str,
    workspace_name: &str,
    workspace_slug: &str,
    emails: &BTreeMap<String, String>,
    role_names: &BTreeMap<String, String>,
) -> String {
    let id = target_id.trim();
    match target_type {
        "organization" => {
            if !workspace_name.trim().is_empty() {
                workspace_name.to_owned()
            } else if !workspace_slug.trim().is_empty() {
                workspace_slug.to_owned()
            } else {
                "Workspace".to_owned()
            }
        }
        "session" => "Session".to_owned(),
        "user" | "membership" | "member" => {
            if let Some(email) = emails.get(id) {
                if !email.trim().is_empty() {
                    return email.clone();
                }
            }
            if id.is_empty() {
                "Member".to_owned()
            } else {
                "Member".to_owned()
            }
        }
        "invitation" => {
            if let Some(email) = emails.get(id) {
                if !email.trim().is_empty() {
                    return email.clone();
                }
            }
            "Invitation".to_owned()
        }
        "role" => {
            if let Some(name) = role_names.get(id) {
                return name.clone();
            }
            if id.is_empty() {
                "Role".to_owned()
            } else {
                // Role ids are human keys (admin, member), not UUIDs.
                id.to_owned()
            }
        }
        "passkey" => "Passkey".to_owned(),
        "signing_key" => "Signing key".to_owned(),
        "policy_bundle" => "Policy bundle".to_owned(),
        "oauth_provider" => "OAuth provider".to_owned(),
        other if other.is_empty() => {
            if id.is_empty() {
                "Resource".to_owned()
            } else if looks_like_uuid(id) {
                "Resource".to_owned()
            } else {
                id.to_owned()
            }
        }
        other => {
            let kind = humanize_target_type(other);
            if id.is_empty() || looks_like_uuid(id) {
                kind
            } else {
                format!("{kind}: {id}")
            }
        }
    }
}

fn looks_like_uuid(value: &str) -> bool {
    let v = value.trim();
    v.len() >= 32
        && v.chars()
            .all(|c| c.is_ascii_hexdigit() || c == '-' || c == '_')
}

fn outcome_class(outcome: &str) -> &'static str {
    match outcome {
        "succeeded" | "allowed" => "ok",
        "failed" => "fail",
        "denied" => "deny",
        _ => "unknown",
    }
}

fn outcome_icon_glyph(outcome: &str) -> &'static str {
    match outcome {
        "succeeded" | "allowed" => "✓",
        "failed" => "✕",
        "denied" => "⊘",
        _ => "·",
    }
}

const KNOWN_AUDIT_ACTIONS: &[&str] = &[
    "organization.create",
    "organization.update",
    "organization.select",
    "organization.view",
    "member.invite",
    "member.invite.resend",
    "member.invite.revoke",
    "member.manage",
    "member.remove",
    "member.leave",
    "member.role.assign",
    "member.view",
    "role.manage",
    "role.view",
    "invitation.accept",
    "ownership.transfer",
    "organization.archive",
    "auth.password.login",
    "auth.password.register",
    "auth.password.change",
    "auth.password.reset.start",
    "auth.password.reset.complete",
    "auth.session.revoke",
    "auth.token.issue",
    "auth.token.refresh",
    "auth.mfa.totp.start",
    "auth.mfa.totp.confirm",
    "auth.mfa.totp.verify",
    "auth.mfa.recovery.verify",
    "auth.passkey.login",
    "auth.passkey.register",
    "auth.oauth.login",
    "auth.email.verify",
    "auth.email.verification.resend",
    "auth.policy.publish",
    "auth.provider.update",
    "auth.signing-key.activate",
    "system.user.disable",
    "system.user.enable",
];

const KNOWN_AUDIT_OUTCOMES: &[&str] = &["succeeded", "failed", "allowed", "denied"];

fn humanize_audit_action(action: &str) -> String {
    match action {
        "organization.create" => "Created workspace".to_owned(),
        "organization.update" => "Updated workspace".to_owned(),
        "organization.select" => "Selected workspace".to_owned(),
        "organization.view" => "Viewed workspace".to_owned(),
        "organization.archive" => "Deactivated workspace".to_owned(),
        "member.invite" => "Invited member".to_owned(),
        "member.invite.resend" => "Resent invitation".to_owned(),
        "member.invite.revoke" => "Revoked invitation".to_owned(),
        "member.manage" => "Managed member".to_owned(),
        "member.remove" => "Removed member".to_owned(),
        "member.leave" => "Left workspace".to_owned(),
        "member.role.assign" => "Changed member role".to_owned(),
        "member.view" => "Viewed members".to_owned(),
        "role.manage" => "Managed role".to_owned(),
        "role.view" => "Viewed roles".to_owned(),
        "invitation.accept" => "Accepted invitation".to_owned(),
        "ownership.transfer" => "Transferred ownership".to_owned(),
        "auth.password.login" => "Password sign-in".to_owned(),
        "auth.password.register" => "Registered account".to_owned(),
        "auth.password.change" => "Changed password".to_owned(),
        "auth.password.reset.start" => "Started password reset".to_owned(),
        "auth.password.reset.complete" => "Completed password reset".to_owned(),
        "auth.session.revoke" => "Revoked session".to_owned(),
        "auth.token.issue" => "Issued token".to_owned(),
        "auth.token.refresh" => "Refreshed token".to_owned(),
        "auth.token.reuse" => "Detected token reuse".to_owned(),
        "auth.mfa.totp.start" => "Started MFA enrollment".to_owned(),
        "auth.mfa.totp.confirm" => "Confirmed MFA enrollment".to_owned(),
        "auth.mfa.totp.verify" => "Verified MFA code".to_owned(),
        "auth.mfa.recovery.verify" => "Used recovery code".to_owned(),
        "auth.passkey.login" => "Passkey sign-in".to_owned(),
        "auth.passkey.register" => "Registered passkey".to_owned(),
        "auth.oauth.login" => "OAuth sign-in".to_owned(),
        "auth.email.verify" => "Verified email".to_owned(),
        "auth.email.verification.resend" => "Resent email verification".to_owned(),
        "auth.policy.publish" => "Published policy".to_owned(),
        "auth.provider.update" => "Updated auth provider".to_owned(),
        "auth.signing-key.activate" => "Activated signing key".to_owned(),
        "system.user.disable" => "Disabled user".to_owned(),
        "system.user.enable" => "Enabled user".to_owned(),
        other => humanize_dotted_identifier(other),
    }
}

fn humanize_audit_outcome(outcome: &str) -> String {
    match outcome {
        "succeeded" => "Succeeded".to_owned(),
        "failed" => "Failed".to_owned(),
        "allowed" => "Allowed".to_owned(),
        "denied" => "Denied".to_owned(),
        other if other.is_empty() => "—".to_owned(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => "—".to_owned(),
            }
        }
    }
}

fn humanize_target_type(target_type: &str) -> String {
    match target_type {
        "organization" => "Workspace".to_owned(),
        "membership" | "member" => "Member".to_owned(),
        "invitation" => "Invitation".to_owned(),
        "role" => "Role".to_owned(),
        "user" => "User".to_owned(),
        "session" => "Session".to_owned(),
        "signing_key" => "Signing key".to_owned(),
        "policy_bundle" => "Policy bundle".to_owned(),
        "oauth_provider" => "OAuth provider".to_owned(),
        "passkey" => "Passkey".to_owned(),
        other if other.is_empty() => "Resource".to_owned(),
        other => humanize_dotted_identifier(other),
    }
}

fn humanize_dotted_identifier(value: &str) -> String {
    value
        .split(|c| c == '.' || c == '_' || c == '-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Pretty JSON of list-API fields only (no invented secrets or missing columns).
fn safe_event_metadata_json(event: &AuditEventSummary) -> String {
    let org = event.organization_id.as_deref().unwrap_or("");
    format!(
        "{{\n  \"sequence\": {},\n  \"organization_id\": {},\n  \"actor_user_id\": {},\n  \"action\": {},\n  \"target_type\": {},\n  \"target_id\": {},\n  \"outcome\": {},\n  \"recorded_at_ms\": {},\n  \"request_id\": null,\n  \"policy_revision\": null,\n  \"metadata\": {{}}\n}}",
        event.sequence,
        json_string(org),
        json_string(&event.actor_user_id),
        json_string(&event.action),
        json_string(&event.target_type),
        json_string(&event.target_id),
        json_string(&event.outcome),
        event.recorded_at_ms,
    )
}

fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
