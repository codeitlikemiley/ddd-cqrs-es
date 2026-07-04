//! Stable storage contract names for authorization services.
//!
//! Runtime applications own adapter-specific schema and projection code.

/// Version marker for the first authorization storage contract.
pub const AUTHZ_STORAGE_VERSION: &str = "authz.v1";

/// Event stream contract for an authorization aggregate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuthzEventStreamContract {
    pub stream_name: &'static str,
    pub aggregate_type: &'static str,
    pub id_prefix: &'static str,
}

/// Read model contract for an authorization projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuthzReadModelContract {
    pub name: &'static str,
    pub primary_key: &'static str,
    pub purpose: &'static str,
}

pub const AUTHZ_MODEL_STREAM: AuthzEventStreamContract = AuthzEventStreamContract {
    stream_name: "authz.models",
    aggregate_type: "authz_model",
    id_prefix: "authz_model",
};

pub const AUTHZ_TUPLE_SET_STREAM: AuthzEventStreamContract = AuthzEventStreamContract {
    stream_name: "authz.tuple_sets",
    aggregate_type: "authz_tuple_set",
    id_prefix: "authz_tuple_set",
};

pub const AUTHZ_ASSERTION_STREAM: AuthzEventStreamContract = AuthzEventStreamContract {
    stream_name: "authz.assertions",
    aggregate_type: "authz_assertion_set",
    id_prefix: "authz_assertion",
};

pub const AUTHZ_STREAMS: &[AuthzEventStreamContract] = &[
    AUTHZ_MODEL_STREAM,
    AUTHZ_TUPLE_SET_STREAM,
    AUTHZ_ASSERTION_STREAM,
];

pub const AUTHZ_ACTIVE_MODEL_READ_MODEL: AuthzReadModelContract = AuthzReadModelContract {
    name: "authz_active_model",
    primary_key: "tenant_id",
    purpose: "active authorization model selected for tenant checks",
};

pub const AUTHZ_RELATIONSHIP_TUPLES_READ_MODEL: AuthzReadModelContract = AuthzReadModelContract {
    name: "authz_relationship_tuples",
    primary_key: "tenant_id, object_ref, relation, subject_ref",
    purpose: "canonical ReBAC relationship tuple projection",
};

pub const AUTHZ_TUPLE_INDEX_BY_SUBJECT_READ_MODEL: AuthzReadModelContract =
    AuthzReadModelContract {
        name: "authz_tuple_index_by_subject",
        primary_key: "tenant_id, subject_ref, relation, object_ref",
        purpose: "list objects and reverse lookup by subject",
    };

pub const AUTHZ_TUPLE_INDEX_BY_OBJECT_READ_MODEL: AuthzReadModelContract = AuthzReadModelContract {
    name: "authz_tuple_index_by_object",
    primary_key: "tenant_id, object_ref, relation, subject_ref",
    purpose: "list users and relationship lookup by object",
};

pub const AUTHZ_CHECK_AUDIT_READ_MODEL: AuthzReadModelContract = AuthzReadModelContract {
    name: "authz_check_audit",
    primary_key: "tenant_id, check_id",
    purpose: "optional decision audit log for sampled authorization checks",
};

pub const AUTHZ_READ_MODELS: &[AuthzReadModelContract] = &[
    AUTHZ_ACTIVE_MODEL_READ_MODEL,
    AUTHZ_RELATIONSHIP_TUPLES_READ_MODEL,
    AUTHZ_TUPLE_INDEX_BY_SUBJECT_READ_MODEL,
    AUTHZ_TUPLE_INDEX_BY_OBJECT_READ_MODEL,
    AUTHZ_CHECK_AUDIT_READ_MODEL,
];

/// Return the stream contract for a known aggregate type.
pub fn authz_stream_contract(aggregate_type: &str) -> Option<&'static AuthzEventStreamContract> {
    AUTHZ_STREAMS
        .iter()
        .find(|contract| contract.aggregate_type == aggregate_type)
}

/// Return the read model contract for a known projection name.
pub fn authz_read_model_contract(name: &str) -> Option<&'static AuthzReadModelContract> {
    AUTHZ_READ_MODELS
        .iter()
        .find(|contract| contract.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn authz_stream_names_are_unique() {
        let mut names = BTreeSet::new();

        for contract in AUTHZ_STREAMS {
            assert!(
                names.insert(contract.stream_name),
                "duplicate stream name: {}",
                contract.stream_name
            );
        }
    }

    #[test]
    fn authz_read_model_names_are_unique() {
        let mut names = BTreeSet::new();

        for contract in AUTHZ_READ_MODELS {
            assert!(
                names.insert(contract.name),
                "duplicate read model name: {}",
                contract.name
            );
        }
    }

    #[test]
    fn lookup_returns_authz_tuple_set_stream_contract() {
        let contract = authz_stream_contract("authz_tuple_set").expect("authz_tuple_set stream");

        assert_eq!(contract.stream_name, AUTHZ_TUPLE_SET_STREAM.stream_name);
        assert_eq!(contract.id_prefix, "authz_tuple_set");
    }
}
