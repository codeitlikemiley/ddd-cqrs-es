use crate::{AuthzError, Relation};
use std::collections::BTreeMap;

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorizationModel {
    pub model_id: String,
    pub schema_version: String,
    pub types: BTreeMap<String, ObjectType>,
}

impl AuthorizationModel {
    pub fn new(model_id: impl Into<String>) -> Self {
        Self {
            model_id: model_id.into(),
            schema_version: "1.0".to_string(),
            types: BTreeMap::new(),
        }
    }

    pub fn with_type(mut self, object_type: ObjectType) -> Self {
        self.types.insert(object_type.name.clone(), object_type);
        self
    }

    pub fn validate(&self) -> Result<(), AuthzError> {
        if self.model_id.trim().is_empty() {
            return Err(AuthzError::validation("model_id must not be empty"));
        }
        if self.schema_version.trim().is_empty() {
            return Err(AuthzError::validation("schema_version must not be empty"));
        }
        if self.types.is_empty() {
            return Err(AuthzError::validation(
                "authorization model must define at least one object type",
            ));
        }

        for (type_name, object_type) in &self.types {
            if type_name.trim().is_empty() || object_type.name.trim().is_empty() {
                return Err(AuthzError::validation("object type name must not be empty"));
            }
            if type_name != &object_type.name {
                return Err(AuthzError::validation(format!(
                    "object type key `{type_name}` does not match object type name `{}`",
                    object_type.name
                )));
            }
            if object_type.relations.is_empty() {
                return Err(AuthzError::validation(format!(
                    "object type `{type_name}` must define at least one relation"
                )));
            }
            for (relation_name, definition) in &object_type.relations {
                if relation_name.trim().is_empty() {
                    return Err(AuthzError::validation(format!(
                        "object type `{type_name}` has an empty relation name"
                    )));
                }
                definition.rewrite.validate(type_name, relation_name)?;
            }
        }

        Ok(())
    }

    #[cfg(feature = "json")]
    pub fn from_json(value: &str) -> Result<Self, AuthzError> {
        let model: Self = serde_json::from_str(value).map_err(|error| {
            AuthzError::validation(format!("authorization model JSON is invalid: {error}"))
        })?;
        model.validate()?;
        Ok(model)
    }

    #[cfg(feature = "json")]
    pub fn to_json(&self) -> Result<String, AuthzError> {
        self.validate()?;
        serde_json::to_string(self).map_err(|error| {
            AuthzError::validation(format!(
                "authorization model JSON serialization failed: {error}"
            ))
        })
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectType {
    pub name: String,
    pub relations: BTreeMap<String, RelationDefinition>,
}

impl ObjectType {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            relations: BTreeMap::new(),
        }
    }

    pub fn with_relation(mut self, relation: Relation, definition: RelationDefinition) -> Self {
        self.relations
            .insert(relation.as_str().to_string(), definition);
        self
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationDefinition {
    pub rewrite: Rewrite,
}

impl RelationDefinition {
    pub fn direct() -> Self {
        Self {
            rewrite: Rewrite::Direct,
        }
    }

    pub fn computed_userset(relation: Relation) -> Self {
        Self {
            rewrite: Rewrite::ComputedUserset { relation },
        }
    }

    pub fn tuple_to_userset(tupleset_relation: Relation, computed_relation: Relation) -> Self {
        Self {
            rewrite: Rewrite::TupleToUserset {
                tupleset_relation,
                computed_relation,
            },
        }
    }

    pub fn union(children: Vec<Rewrite>) -> Self {
        Self {
            rewrite: Rewrite::Union { children },
        }
    }

    pub fn intersection(children: Vec<Rewrite>) -> Self {
        Self {
            rewrite: Rewrite::Intersection { children },
        }
    }

    pub fn exclusion(base: Rewrite, subtract: Rewrite) -> Self {
        Self {
            rewrite: Rewrite::Exclusion {
                base: Box::new(base),
                subtract: Box::new(subtract),
            },
        }
    }

    pub fn condition(name: impl Into<String>) -> Self {
        Self {
            rewrite: Rewrite::Condition { name: name.into() },
        }
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "serde", serde(tag = "type", rename_all = "snake_case"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rewrite {
    Direct,
    ComputedUserset {
        relation: Relation,
    },
    TupleToUserset {
        tupleset_relation: Relation,
        computed_relation: Relation,
    },
    Union {
        children: Vec<Rewrite>,
    },
    Intersection {
        children: Vec<Rewrite>,
    },
    Exclusion {
        base: Box<Rewrite>,
        subtract: Box<Rewrite>,
    },
    Condition {
        name: String,
    },
}

impl Rewrite {
    pub fn direct() -> Self {
        Self::Direct
    }

    pub fn computed_userset(relation: Relation) -> Self {
        Self::ComputedUserset { relation }
    }

    pub fn tuple_to_userset(tupleset_relation: Relation, computed_relation: Relation) -> Self {
        Self::TupleToUserset {
            tupleset_relation,
            computed_relation,
        }
    }

    pub fn union(children: Vec<Rewrite>) -> Self {
        Self::Union { children }
    }

    pub fn intersection(children: Vec<Rewrite>) -> Self {
        Self::Intersection { children }
    }

    pub fn exclusion(base: Rewrite, subtract: Rewrite) -> Self {
        Self::Exclusion {
            base: Box::new(base),
            subtract: Box::new(subtract),
        }
    }

    pub fn condition(name: impl Into<String>) -> Self {
        Self::Condition { name: name.into() }
    }

    fn validate(&self, type_name: &str, relation_name: &str) -> Result<(), AuthzError> {
        match self {
            Self::Direct => Ok(()),
            Self::ComputedUserset { relation } => validate_relation_ref(
                type_name,
                relation_name,
                "computed userset relation",
                relation,
            ),
            Self::TupleToUserset {
                tupleset_relation,
                computed_relation,
            } => {
                validate_relation_ref(
                    type_name,
                    relation_name,
                    "tuple-to-userset tupleset relation",
                    tupleset_relation,
                )?;
                validate_relation_ref(
                    type_name,
                    relation_name,
                    "tuple-to-userset computed relation",
                    computed_relation,
                )
            }
            Self::Union { children } | Self::Intersection { children } => {
                if children.is_empty() {
                    return Err(AuthzError::validation(format!(
                        "{type_name}.{relation_name} rewrite must have at least one child"
                    )));
                }
                for child in children {
                    child.validate(type_name, relation_name)?;
                }
                Ok(())
            }
            Self::Exclusion { base, subtract } => {
                base.validate(type_name, relation_name)?;
                subtract.validate(type_name, relation_name)
            }
            Self::Condition { name } => {
                if name.trim().is_empty() {
                    return Err(AuthzError::validation(format!(
                        "{type_name}.{relation_name} condition name must not be empty"
                    )));
                }
                Ok(())
            }
        }
    }
}

fn validate_relation_ref(
    type_name: &str,
    relation_name: &str,
    label: &str,
    relation: &Relation,
) -> Result<(), AuthzError> {
    if relation.as_str().trim().is_empty() {
        return Err(AuthzError::validation(format!(
            "{type_name}.{relation_name} {label} must not be empty"
        )));
    }
    Ok(())
}
