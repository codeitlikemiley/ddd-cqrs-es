//! gRPC service surface for Spin (auth, authorization, organization, admin, audit).

pub mod auth_proto {
    tonic::include_proto!("auth.v1");
}

pub mod authorization_proto {
    tonic::include_proto!("authorization.v1");
}

pub mod organization_proto {
    tonic::include_proto!("organization.v1");
}

pub mod admin_proto {
    tonic::include_proto!("admin.v1");
}

pub mod audit_proto {
    tonic::include_proto!("audit.v1");
}

mod admin;
mod audit;
mod auth;
mod authorization;
mod convert;
mod organization;
mod serve;

pub use serve::{is_grpc_request, serve};

// Service modules only contain trait impls (no free items). Re-export helpers
// and serve-side types so `use super::*` in those modules resolves cleanly.
pub(crate) use convert::*;
pub(crate) use serve::*;
