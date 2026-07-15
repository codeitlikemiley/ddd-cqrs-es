//! Product storage adapters (KV, SQL, vault, dashboard data).
#![allow(unused_imports)]
#![allow(dead_code)]

use std::borrow::Cow;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "mail-capture")]
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use futures::lock::Mutex;
use serde_json::Value;
#[cfg(feature = "mail-capture")]
use serde_json::json;
use sha2::{Digest, Sha256};
#[cfg(all(feature = "spicedb", runtime_spin))]
use wasi_auth::authorization::{
    AccessRequest as SpiceDbAccessRequest, ActionName as SpiceDbActionName,
    Authorizer as SpiceDbAuthorizer, ConsistencyRequirement, Decision as AuthorizationDecision,
    Resource as SpiceDbResource, ResourceType as SpiceDbResourceType,
};
#[cfg(all(feature = "spicedb", runtime_spin))]
use wasi_auth::context::OrganizationId;
#[cfg(all(feature = "spicedb", runtime_spin))]
use wasi_auth::postgres::outbox::load_relationship_consistency;
use wasi_auth::schema::{AppliedSchemaMigration, plan_schema};
#[cfg(all(feature = "spicedb", runtime_spin))]
use wasi_auth::spicedb::{
    PermissionMap, SpiceDbBearerToken, SpiceDbEndpoint, SpiceDbProvider, SpiceDbTransport,
};

use crate::contracts::{
    HealthStatusResponse, StorageEventTypeCount, StorageProjectionRunResponse,
    StorageStatusResponse,
};
use crate::error::{AuthStackError, AuthStackResult};

use super::*;

pub(crate) const DASHBOARD_LAYOUT_ORG_PREFIX: &str = "app_dashboard:layout:org:";
pub(crate) const DASHBOARD_LAYOUT_LEGACY_PREFIX: &str = "app_dashboard:layout:";
pub(crate) const DASHBOARD_NOTIFS_PREFIX: &str = "app_dashboard:notifs:";
pub(crate) const DASHBOARD_SOURCES_PREFIX: &str = "app_dashboard:sources:";
/// Org-scoped vault: `app_dashboard:secrets:org:{organization_id}`
pub(crate) const DASHBOARD_SECRETS_ORG_PREFIX: &str = "app_dashboard:secrets:org:";
/// Legacy per-user vault (migrated on read when possible).
pub(crate) const DASHBOARD_SECRETS_LEGACY_PREFIX: &str = "app_dashboard:secrets:";
pub(crate) const DASHBOARD_RESOURCES_ORG_PREFIX: &str = "app_dashboard:resources:org:";
pub(crate) const DASHBOARD_RESOURCES_LEGACY_PREFIX: &str = "app_dashboard:resources:";
pub(crate) const DASHBOARD_QUERIES_ORG_PREFIX: &str = "app_dashboard:queries:org:";
pub(crate) const DASHBOARD_QUERIES_LEGACY_PREFIX: &str = "app_dashboard:queries:";
pub(crate) const ORG_SLUG_PREFIX: &str = "app_org:slug:";
pub(crate) const ORG_ID_SLUG_PREFIX: &str = "app_org:id:";
pub(crate) const MAX_BOARD_NODES: usize = 48;
pub(crate) const MAX_HTTP_SOURCES: usize = 16;
pub(crate) const MAX_RESOURCES: usize = 32;
pub(crate) const MAX_QUERIES: usize = 48;
pub(crate) const MAX_HTTP_RESPONSE_BYTES: usize = 256 * 1024;

pub(crate) fn dashboard_layout_key(org_id: &str) -> String {
    format!("{DASHBOARD_LAYOUT_ORG_PREFIX}{org_id}")
}

pub(crate) fn dashboard_layout_legacy_user_key(user_id: &str) -> String {
    format!("{DASHBOARD_LAYOUT_LEGACY_PREFIX}{user_id}")
}

pub(crate) fn dashboard_notifs_key(user_id: &str) -> String {
    format!("{DASHBOARD_NOTIFS_PREFIX}{user_id}")
}

pub(crate) fn dashboard_sources_key(user_id: &str) -> String {
    format!("{DASHBOARD_SOURCES_PREFIX}{user_id}")
}

pub(crate) fn dashboard_secrets_key(org_id: &str) -> String {
    format!("{DASHBOARD_SECRETS_ORG_PREFIX}{org_id}")
}

pub(crate) fn dashboard_secrets_legacy_user_key(user_id: &str) -> String {
    format!("{DASHBOARD_SECRETS_LEGACY_PREFIX}{user_id}")
}

pub(crate) fn dashboard_resources_key(org_id: &str) -> String {
    format!("{DASHBOARD_RESOURCES_ORG_PREFIX}{org_id}")
}

pub(crate) fn dashboard_resources_legacy_user_key(user_id: &str) -> String {
    format!("{DASHBOARD_RESOURCES_LEGACY_PREFIX}{user_id}")
}

pub(crate) fn dashboard_queries_key(org_id: &str) -> String {
    format!("{DASHBOARD_QUERIES_ORG_PREFIX}{org_id}")
}

pub(crate) fn dashboard_queries_legacy_user_key(user_id: &str) -> String {
    format!("{DASHBOARD_QUERIES_LEGACY_PREFIX}{user_id}")
}

pub(crate) fn org_slug_key(slug: &str) -> String {
    format!("{ORG_SLUG_PREFIX}{}", slug.trim().to_ascii_lowercase())
}

pub(crate) fn org_id_slug_key(org_id: &str) -> String {
    format!("{ORG_ID_SLUG_PREFIX}{org_id}:slug")
}
