use crate::{ObjectRef, Relation, SubjectRef, TenantRef};

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationshipTuple {
    pub subject: SubjectRef,
    pub relation: Relation,
    pub object: ObjectRef,
    pub condition: Option<String>,
    pub tenant_id: Option<TenantRef>,
}

impl RelationshipTuple {
    pub fn new(subject: SubjectRef, relation: Relation, object: ObjectRef) -> Self {
        Self {
            subject,
            relation,
            object,
            condition: None,
            tenant_id: None,
        }
    }

    pub fn with_condition(mut self, condition: impl Into<String>) -> Self {
        self.condition = Some(condition.into());
        self
    }

    pub fn with_tenant(mut self, tenant_id: TenantRef) -> Self {
        self.tenant_id = Some(tenant_id);
        self
    }
}
