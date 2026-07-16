//! Shared helpers for workspace settings pages.

#![allow(dead_code)]
#![allow(unused_imports)]

use crate::app::helpers::redirect_browser;
use crate::app::{browser_load, get_current_session, list_organizations};
use crate::contracts::OrganizationSummary;
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

/// Route param `slug` for `/org/:slug/settings/…`.
pub(crate) fn settings_slug_from_params() -> Memo<String> {
    let params = use_params_map();
    Memo::new(move |_| {
        params
            .get()
            .get("slug")
            .map(|v| v.to_string())
            .unwrap_or_default()
    })
}

/// Settings section keys (path segment after `/settings/`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SettingsSection {
    General,
    Members,
    Invitations,
    Roles,
    Audit,
    Danger,
}

impl SettingsSection {
    pub(crate) fn path_segment(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Members => "members",
            Self::Invitations => "invitations",
            Self::Roles => "roles",
            Self::Audit => "audit",
            Self::Danger => "danger",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Members => "Members",
            Self::Invitations => "Invitations",
            Self::Roles => "Roles",
            Self::Audit => "Audit log",
            Self::Danger => "Danger zone",
        }
    }

    pub(crate) fn from_path(path: &str) -> Option<Self> {
        let path = path.trim_end_matches('/');
        let Some((_, after)) = path.split_once("/settings") else {
            return None;
        };
        match after.trim_start_matches('/') {
            "" | "general" => Some(Self::General),
            "members" => Some(Self::Members),
            "invitations" => Some(Self::Invitations),
            "roles" => Some(Self::Roles),
            "audit" => Some(Self::Audit),
            "danger" => Some(Self::Danger),
            _ => None,
        }
    }

    pub(crate) fn all() -> &'static [Self] {
        &[
            Self::General,
            Self::Members,
            Self::Invitations,
            Self::Roles,
            Self::Audit,
            Self::Danger,
        ]
    }

    pub(crate) fn href(self, slug: &str) -> String {
        format!("/org/{slug}/settings/{}", self.path_segment())
    }
}

/// Pick active tenant slug, else first org with a slug.
pub(crate) fn resolve_workspace_settings_slug(
    orgs: &[OrganizationSummary],
    tenant_id: Option<&str>,
) -> Option<String> {
    if let Some(tid) = tenant_id.filter(|v| !v.trim().is_empty()) {
        if let Some(org) = orgs
            .iter()
            .find(|o| o.organization_id == tid && !o.slug.is_empty())
        {
            return Some(org.slug.clone());
        }
    }
    orgs.iter()
        .find(|o| !o.slug.is_empty())
        .map(|o| o.slug.clone())
}

/// Redirect legacy `/organizations/{section}` → `/org/{slug}/settings/{section}`.
///
/// Island: client-only Effect (islands hydrate outside the route tree).
#[island]
pub fn LegacySettingsRedirect(section: String) -> impl IntoView {
    let section = StoredValue::new(section);
    let orgs = browser_load(list_organizations);
    let session = browser_load(get_current_session);

    Effect::new(move |_| {
        let orgs_ready = orgs.get();
        let session_ready = session.get();
        let (Some(orgs_result), Some(session_result)) = (orgs_ready, session_ready) else {
            return;
        };

        let tenant_id = session_result
            .ok()
            .and_then(|s| s.tenant_id)
            .filter(|v| !v.trim().is_empty());

        match orgs_result {
            Ok(list) if list.organizations.is_empty() => {
                redirect_browser("/organizations");
            }
            Ok(list) => {
                match resolve_workspace_settings_slug(
                    &list.organizations,
                    tenant_id.as_deref(),
                ) {
                    Some(slug) => {
                        let section = section.get_value();
                        redirect_browser(&format!("/org/{slug}/settings/{section}"));
                    }
                    None => {
                        redirect_browser("/organizations");
                    }
                }
            }
            Err(_) => {
                redirect_browser("/organizations");
            }
        }
    });

    view! {
        <section class="panel workspace-settings-redirect">
            <p class="result-line">"Opening workspace settings…"</p>
            <p class="board-muted">
                <a href="/organizations">"Back to workspaces"</a>
                " if this takes too long."
            </p>
        </section>
    }
}

/// Shared page chrome for settings stubs (title + copy + placeholder body).
pub(crate) fn settings_page_stub(
    title: &'static str,
    subtitle: &'static str,
    permission_hint: &'static str,
    children: impl IntoView + 'static,
) -> impl IntoView {
    view! {
        <div class="workspace-settings-page">
            <header class="workspace-settings-page-header">
                <h1>{title}</h1>
                <p class="workspace-settings-page-sub">{subtitle}</p>
            </header>
            <section class="panel workspace-settings-stub">
                <p class="result-line">{permission_hint}</p>
                {children}
            </section>
        </div>
    }
}
