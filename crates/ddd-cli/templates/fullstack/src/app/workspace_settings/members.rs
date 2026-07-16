//! Workspace settings — Members (list table; full editors in PR4a).

#![allow(unused_imports)]
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]

use super::shared::{display_role_name, settings_page_stub, slug_from_settings_pathname};
use crate::app::helpers::{current_browser_pathname, server_error_text, short_id_label};
use crate::app::{browser_load, list_workspace_members};
use crate::contracts::MembershipSummary;
use leptos::prelude::*;

#[component]
pub fn WorkspaceSettingsMembersPage() -> impl IntoView {
    settings_page_stub(
        "Members",
        "People who belong to this workspace and their roles.",
        "Requires member.view. Role assignment and remove confirmations ship in a later release.",
        view! { <WorkspaceSettingsMembersBody /> },
    )
}

/// Island: load members for the URL slug.
#[island]
pub fn WorkspaceSettingsMembersBody() -> impl IntoView {
    let members = browser_load({
        move || {
            let slug = slug_from_settings_pathname(&current_browser_pathname());
            list_workspace_members(slug)
        }
    });

    let load_error = Memo::new(move |_| {
        members.get().and_then(|result| match result {
            Ok(_) => None,
            Err(error) => Some(server_error_text(error)),
        })
    });

    view! {
        <Show when=move || members.get().is_none()>
            <p class="result-line" aria-busy="true">"Loading members…"</p>
        </Show>

        <Show when=move || load_error.get().is_some()>
            <p class="error-banner">{move || load_error.get().unwrap_or_default()}</p>
        </Show>

        {move || {
            match members.get() {
                Some(Ok(list)) if list.memberships.is_empty() => view! {
                    <div class="workspace-settings-empty" role="status">
                        <p>"No members found."</p>
                    </div>
                }
                .into_any(),
                Some(Ok(list)) => view! {
                    <div class="table-wrap workspace-settings-table-wrap">
                        <table class="data-table workspace-settings-members-table">
                            <thead>
                                <tr>
                                    <th scope="col">"Email"</th>
                                    <th scope="col">"Role"</th>
                                    <th scope="col">"Status"</th>
                                    <th scope="col">"User"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {list
                                    .memberships
                                    .into_iter()
                                    .map(member_row)
                                    .collect_view()}
                            </tbody>
                        </table>
                    </div>
                }
                .into_any(),
                _ => view! { <></> }.into_any(),
            }
        }}
    }
}

fn member_row(member: MembershipSummary) -> impl IntoView {
    let email = if member.primary_email.trim().is_empty() {
        "—".to_owned()
    } else {
        member.primary_email.clone()
    };
    let role = display_role_name(&member.role_id);
    let status = member.status.clone();
    let user_label = short_id_label(&member.user_id);
    view! {
        <tr>
            <td>{email}</td>
            <td>{role}</td>
            <td>{status}</td>
            <td class="mono-value">{user_label}</td>
        </tr>
    }
}
