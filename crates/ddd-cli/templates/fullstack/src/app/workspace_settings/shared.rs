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

/// Parse `/org/{slug}/settings…` from a browser pathname (islands have no Router).
pub(crate) fn slug_from_settings_pathname(path: &str) -> String {
    let path = path.trim_end_matches('/');
    let Some(rest) = path.strip_prefix("/org/") else {
        return String::new();
    };
    let Some((slug, after)) = rest.split_once('/') else {
        return String::new();
    };
    if after == "settings" || after.starts_with("settings/") {
        slug.to_owned()
    } else {
        String::new()
    }
}

/// Display label for a role id when role catalog is unavailable.
pub(crate) fn display_role_name(role_id: &str) -> String {
    match role_id {
        "owner" => "Owner".to_owned(),
        "admin" => "Admin".to_owned(),
        "member" => "Member".to_owned(),
        "viewer" => "Viewer".to_owned(),
        other => other.to_owned(),
    }
}

/// Prefer server-authored role option labels; fall back to built-in names.
pub(crate) fn role_label_from_options(
    role_id: &str,
    options: &[crate::contracts::WorkspaceRoleOption],
) -> String {
    options
        .iter()
        .find(|opt| opt.role_id == role_id)
        .map(|opt| {
            if opt.name.trim().is_empty() {
                display_role_name(role_id)
            } else {
                opt.name.clone()
            }
        })
        .unwrap_or_else(|| display_role_name(role_id))
}

/// Compact timestamp for settings tables (joined / created).
pub(crate) fn format_settings_timestamp_ms(ms: u64) -> String {
    if ms == 0 {
        return "—".to_owned();
    }
    #[cfg(feature = "hydrate")]
    {
        use wasm_bindgen::JsValue;
        let date = js_sys::Date::new(&JsValue::from_f64(ms as f64));
        date.to_locale_date_string("en-CA", &JsValue::UNDEFINED)
            .as_string()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format!("{ms}"))
    }
    #[cfg(not(feature = "hydrate"))]
    {
        // Stable SSR placeholder; hydrate replaces with locale date.
        let days = ms / 86_400_000;
        let year = 1970 + (days / 365);
        format!("~{year}")
    }
}

/// Local date + time for audit log rows (hydrate).
pub(crate) fn format_settings_datetime_ms(ms: u64) -> String {
    if ms == 0 {
        return "—".to_owned();
    }
    #[cfg(feature = "hydrate")]
    {
        use wasm_bindgen::JsValue;
        let date = js_sys::Date::new(&JsValue::from_f64(ms as f64));
        date.to_locale_string("en-CA", &JsValue::UNDEFINED)
            .as_string()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format!("{ms}"))
    }
    #[cfg(not(feature = "hydrate"))]
    {
        let days = ms / 86_400_000;
        let year = 1970 + (days / 365);
        format!("~{year}")
    }
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
