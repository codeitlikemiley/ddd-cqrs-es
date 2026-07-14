//! Thin Spin runtime adapter for product workflows owned by `wasi-auth`.
//! Domain modules re-exported so existing `crate::auth_product::*` call sites keep working.

mod admin;
mod config;
mod errors;
mod flows;
mod infra;
mod organization;
mod password;
mod providers;
mod runtime;
mod session;

pub(crate) use admin::*;
pub(crate) use config::*;
pub(crate) use errors::*;
pub(crate) use flows::*;
pub(crate) use infra::*;
pub(crate) use organization::*;
pub(crate) use password::*;
pub(crate) use providers::*;
pub(crate) use runtime::*;
pub(crate) use session::*;
