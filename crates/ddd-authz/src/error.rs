use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthzError {
    Validation { message: String },
    UnknownModel,
    UnknownObjectType { object_type: String },
    UnknownRelation { relation: String },
    MaxDepthExceeded,
    MaxNodesExceeded,
    CycleDetected,
}

impl AuthzError {
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation {
            message: message.into(),
        }
    }

    pub fn public_code(&self) -> &'static str {
        match self {
            Self::Validation { .. } => "validation",
            Self::UnknownModel => "unknown_model",
            Self::UnknownObjectType { .. } => "unknown_object_type",
            Self::UnknownRelation { .. } => "unknown_relation",
            Self::MaxDepthExceeded => "max_depth_exceeded",
            Self::MaxNodesExceeded => "max_nodes_exceeded",
            Self::CycleDetected => "cycle_detected",
        }
    }
}

impl Display for AuthzError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Validation { message } => f.write_str(message),
            Self::UnknownModel => f.write_str("authorization model is unknown"),
            Self::UnknownObjectType { object_type } => {
                write!(f, "object type `{object_type}` is unknown")
            }
            Self::UnknownRelation { relation } => write!(f, "relation `{relation}` is unknown"),
            Self::MaxDepthExceeded => f.write_str("authorization check exceeded max depth"),
            Self::MaxNodesExceeded => f.write_str("authorization check exceeded max node count"),
            Self::CycleDetected => f.write_str("authorization model contains a relation cycle"),
        }
    }
}

impl Error for AuthzError {}
