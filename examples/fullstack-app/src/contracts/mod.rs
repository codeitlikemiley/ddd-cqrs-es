#![allow(dead_code)]

//! Shared wire/DTO contracts for the fullstack app.
//! Split by domain; this module re-exports everything so existing `crate::contracts::*` keeps working.

mod admin;
mod auth;
mod dashboard;
mod organization;
mod profile;
mod resources;
mod vault;

pub use admin::*;
pub use auth::*;
pub use dashboard::*;
pub use organization::*;
pub use profile::*;
pub use resources::*;
pub use vault::*;
