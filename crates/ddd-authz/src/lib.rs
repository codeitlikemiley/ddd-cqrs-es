//! Authorization primitives for `ddd_cqrs_es` applications.
//!
//! This crate models RBAC, ReBAC, and bounded ABAC concepts without owning any
//! HTTP, gRPC, Spin, or Leptos adapters.

mod error;
mod evaluator;
mod model;
mod storage;
mod tuple;
mod types;

pub use error::AuthzError;
pub use evaluator::{AuthzContext, Decision, Evaluator};
pub use model::{AuthorizationModel, ObjectType, RelationDefinition, Rewrite};
pub use storage::{
    authz_read_model_contract, authz_stream_contract, AuthzEventStreamContract,
    AuthzReadModelContract, AUTHZ_ACTIVE_MODEL_READ_MODEL, AUTHZ_ASSERTION_STREAM,
    AUTHZ_CHECK_AUDIT_READ_MODEL, AUTHZ_MODEL_STREAM, AUTHZ_READ_MODELS,
    AUTHZ_RELATIONSHIP_TUPLES_READ_MODEL, AUTHZ_STORAGE_VERSION, AUTHZ_STREAMS,
    AUTHZ_TUPLE_INDEX_BY_OBJECT_READ_MODEL, AUTHZ_TUPLE_INDEX_BY_SUBJECT_READ_MODEL,
    AUTHZ_TUPLE_SET_STREAM,
};
pub use tuple::RelationshipTuple;
pub use types::{ObjectRef, Relation, SubjectRef, TenantRef};
