use crate::{
    AuthorizationModel, AuthzError, ObjectRef, Relation, RelationshipTuple, Rewrite, SubjectRef,
    TenantRef,
};
use std::collections::{BTreeMap, BTreeSet};

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuthzContext {
    pub tenant_id: Option<TenantRef>,
    pub attributes: BTreeMap<String, String>,
    pub token_claims: BTreeMap<String, String>,
    pub contextual_tuples: Vec<RelationshipTuple>,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    pub allowed: bool,
    pub model_id: String,
    pub reason: Option<String>,
    pub visited: usize,
}

impl Decision {
    pub fn allow(model_id: impl Into<String>, visited: usize) -> Self {
        Self {
            allowed: true,
            model_id: model_id.into(),
            reason: None,
            visited,
        }
    }

    pub fn deny(model_id: impl Into<String>, reason: impl Into<String>, visited: usize) -> Self {
        Self {
            allowed: false,
            model_id: model_id.into(),
            reason: Some(reason.into()),
            visited,
        }
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpansionNode {
    pub object: ObjectRef,
    pub relation: Relation,
    pub rewrite: String,
    pub subjects: Vec<SubjectRef>,
    pub children: Vec<ExpansionNode>,
    pub visited: usize,
}

#[derive(Clone, Debug)]
pub struct Evaluator {
    model: AuthorizationModel,
    tuples: Vec<RelationshipTuple>,
    max_depth: usize,
    max_nodes: usize,
}

impl Evaluator {
    pub fn new(model: AuthorizationModel, tuples: Vec<RelationshipTuple>) -> Self {
        Self {
            model,
            tuples,
            max_depth: 32,
            max_nodes: 1024,
        }
    }

    pub fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }

    pub fn with_max_nodes(mut self, max_nodes: usize) -> Self {
        self.max_nodes = max_nodes;
        self
    }

    pub fn check(
        &self,
        subject: &SubjectRef,
        relation: &Relation,
        object: &ObjectRef,
        context: &AuthzContext,
    ) -> Result<Decision, AuthzError> {
        self.ensure_relation(object, relation)?;
        let mut state = EvaluationState::default();
        let allowed = self.check_inner(subject, relation, object, context, &mut state, 0)?;

        if allowed {
            Ok(Decision::allow(self.model.model_id.clone(), state.visited))
        } else {
            Ok(Decision::deny(
                self.model.model_id.clone(),
                "no matching relationship path",
                state.visited,
            ))
        }
    }

    pub fn batch_check(
        &self,
        checks: &[(&SubjectRef, &Relation, &ObjectRef)],
        context: &AuthzContext,
    ) -> Result<Vec<Decision>, AuthzError> {
        checks
            .iter()
            .map(|(subject, relation, object)| self.check(subject, relation, object, context))
            .collect()
    }

    pub fn list_objects(
        &self,
        subject: &SubjectRef,
        relation: &Relation,
        object_type: &str,
        context: &AuthzContext,
    ) -> Result<Vec<ObjectRef>, AuthzError> {
        let object_definition =
            self.model
                .types
                .get(object_type)
                .ok_or_else(|| AuthzError::UnknownObjectType {
                    object_type: object_type.to_string(),
                })?;
        if !object_definition.relations.contains_key(relation.as_str()) {
            return Err(AuthzError::UnknownRelation {
                relation: relation.as_str().to_string(),
            });
        }

        let mut candidates = BTreeSet::new();
        for tuple in self.all_tuples(context) {
            if tuple.object.type_name() == object_type {
                candidates.insert(tuple.object.clone());
            }
        }

        let mut objects = Vec::new();
        for object in candidates {
            if self.check(subject, relation, &object, context)?.allowed {
                objects.push(object);
            }
        }
        Ok(objects)
    }

    pub fn expand(
        &self,
        relation: &Relation,
        object: &ObjectRef,
        context: &AuthzContext,
    ) -> Result<ExpansionNode, AuthzError> {
        self.ensure_relation(object, relation)?;
        let mut state = EvaluationState::default();
        self.expand_inner(relation, object, context, &mut state, 0)
    }

    fn check_inner(
        &self,
        subject: &SubjectRef,
        relation: &Relation,
        object: &ObjectRef,
        context: &AuthzContext,
        state: &mut EvaluationState,
        depth: usize,
    ) -> Result<bool, AuthzError> {
        let key = EvalKey::new(subject, relation, object);
        if let Some(allowed) = state.memo.get(&key) {
            return Ok(*allowed);
        }
        self.enter(&key, state, depth)?;

        let result = (|| {
            let definition = self.relation_definition(object, relation)?;
            self.eval_rewrite(
                subject,
                relation,
                object,
                &definition.rewrite,
                context,
                state,
                depth,
            )
        })();

        state.stack.remove(&key);
        let allowed = result?;
        state.memo.insert(key, allowed);
        Ok(allowed)
    }

    fn eval_rewrite(
        &self,
        subject: &SubjectRef,
        relation: &Relation,
        object: &ObjectRef,
        rewrite: &Rewrite,
        context: &AuthzContext,
        state: &mut EvaluationState,
        depth: usize,
    ) -> Result<bool, AuthzError> {
        match rewrite {
            Rewrite::Direct => Ok(self.direct_match(subject, relation, object, context)),
            Rewrite::ComputedUserset {
                relation: computed_relation,
            } => self.check_inner(
                subject,
                computed_relation,
                object,
                context,
                state,
                depth + 1,
            ),
            Rewrite::TupleToUserset {
                tupleset_relation,
                computed_relation,
            } => {
                for tuple in self.all_tuples(context).into_iter().filter(|tuple| {
                    tuple.object == *object
                        && tuple.relation == *tupleset_relation
                        && self.tuple_tenant_matches(tuple, context)
                        && self.tuple_condition_matches(tuple, context)
                }) {
                    let related_object = ObjectRef::new(tuple.subject.as_str().to_string())?;
                    if self.check_inner(
                        subject,
                        computed_relation,
                        &related_object,
                        context,
                        state,
                        depth + 1,
                    )? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Rewrite::Union { children } => {
                for child in children {
                    if self.eval_rewrite(
                        subject,
                        relation,
                        object,
                        child,
                        context,
                        state,
                        depth + 1,
                    )? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Rewrite::Intersection { children } => {
                for child in children {
                    if !self.eval_rewrite(
                        subject,
                        relation,
                        object,
                        child,
                        context,
                        state,
                        depth + 1,
                    )? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Rewrite::Exclusion { base, subtract } => {
                let base_allowed =
                    self.eval_rewrite(subject, relation, object, base, context, state, depth + 1)?;
                if !base_allowed {
                    return Ok(false);
                }
                let subtract_allowed = self.eval_rewrite(
                    subject,
                    relation,
                    object,
                    subtract,
                    context,
                    state,
                    depth + 1,
                )?;
                Ok(!subtract_allowed)
            }
            Rewrite::Condition { name } => Ok(condition_matches(name, context)),
        }
    }

    fn direct_match(
        &self,
        subject: &SubjectRef,
        relation: &Relation,
        object: &ObjectRef,
        context: &AuthzContext,
    ) -> bool {
        self.all_tuples(context).into_iter().any(|tuple| {
            tuple.subject == *subject
                && tuple.relation == *relation
                && tuple.object == *object
                && self.tuple_tenant_matches(tuple, context)
                && self.tuple_condition_matches(tuple, context)
        })
    }

    fn tuple_tenant_matches(&self, tuple: &RelationshipTuple, context: &AuthzContext) -> bool {
        match (&context.tenant_id, &tuple.tenant_id) {
            (Some(expected), Some(actual)) => expected == actual,
            (Some(_), None) => true,
            (None, _) => true,
        }
    }

    fn tuple_condition_matches(&self, tuple: &RelationshipTuple, context: &AuthzContext) -> bool {
        tuple
            .condition
            .as_deref()
            .map(|condition| condition_matches(condition, context))
            .unwrap_or(true)
    }

    fn relation_definition(
        &self,
        object: &ObjectRef,
        relation: &Relation,
    ) -> Result<&crate::RelationDefinition, AuthzError> {
        let object_type = object.type_name();
        let definition_type =
            self.model
                .types
                .get(object_type)
                .ok_or_else(|| AuthzError::UnknownObjectType {
                    object_type: object_type.to_string(),
                })?;
        definition_type
            .relations
            .get(relation.as_str())
            .ok_or_else(|| AuthzError::UnknownRelation {
                relation: relation.as_str().to_string(),
            })
    }

    fn ensure_relation(&self, object: &ObjectRef, relation: &Relation) -> Result<(), AuthzError> {
        self.relation_definition(object, relation).map(|_| ())
    }

    fn all_tuples<'a>(&'a self, context: &'a AuthzContext) -> Vec<&'a RelationshipTuple> {
        self.tuples
            .iter()
            .chain(context.contextual_tuples.iter())
            .collect()
    }

    fn enter(
        &self,
        key: &EvalKey,
        state: &mut EvaluationState,
        depth: usize,
    ) -> Result<(), AuthzError> {
        if depth > self.max_depth {
            return Err(AuthzError::MaxDepthExceeded);
        }
        if state.visited >= self.max_nodes {
            return Err(AuthzError::MaxNodesExceeded);
        }
        if state.stack.contains(key) {
            return Err(AuthzError::CycleDetected);
        }
        state.visited += 1;
        state.stack.insert(key.clone());
        Ok(())
    }

    fn expand_inner(
        &self,
        relation: &Relation,
        object: &ObjectRef,
        context: &AuthzContext,
        state: &mut EvaluationState,
        depth: usize,
    ) -> Result<ExpansionNode, AuthzError> {
        let synthetic_subject = SubjectRef::unchecked("__expand__:subject");
        let key = EvalKey::new(&synthetic_subject, relation, object);
        self.enter(&key, state, depth)?;

        let result = (|| {
            let definition = self.relation_definition(object, relation)?;
            self.expand_rewrite(relation, object, &definition.rewrite, context, state, depth)
        })();

        state.stack.remove(&key);
        result
    }

    fn expand_rewrite(
        &self,
        relation: &Relation,
        object: &ObjectRef,
        rewrite: &Rewrite,
        context: &AuthzContext,
        state: &mut EvaluationState,
        depth: usize,
    ) -> Result<ExpansionNode, AuthzError> {
        let mut node = ExpansionNode {
            object: object.clone(),
            relation: relation.clone(),
            rewrite: rewrite_name(rewrite).to_string(),
            subjects: Vec::new(),
            children: Vec::new(),
            visited: state.visited,
        };

        match rewrite {
            Rewrite::Direct => {
                let mut subjects = BTreeSet::new();
                for tuple in self.all_tuples(context).into_iter().filter(|tuple| {
                    tuple.object == *object
                        && tuple.relation == *relation
                        && self.tuple_tenant_matches(tuple, context)
                        && self.tuple_condition_matches(tuple, context)
                }) {
                    subjects.insert(tuple.subject.clone());
                }
                node.subjects = subjects.into_iter().collect();
            }
            Rewrite::ComputedUserset {
                relation: computed_relation,
            } => {
                node.children.push(self.expand_inner(
                    computed_relation,
                    object,
                    context,
                    state,
                    depth + 1,
                )?);
            }
            Rewrite::TupleToUserset {
                tupleset_relation,
                computed_relation,
            } => {
                for tuple in self.all_tuples(context).into_iter().filter(|tuple| {
                    tuple.object == *object
                        && tuple.relation == *tupleset_relation
                        && self.tuple_tenant_matches(tuple, context)
                        && self.tuple_condition_matches(tuple, context)
                }) {
                    let related_object = ObjectRef::new(tuple.subject.as_str().to_string())?;
                    node.children.push(self.expand_inner(
                        computed_relation,
                        &related_object,
                        context,
                        state,
                        depth + 1,
                    )?);
                }
            }
            Rewrite::Union { children } | Rewrite::Intersection { children } => {
                for child in children {
                    node.children.push(self.expand_rewrite(
                        relation,
                        object,
                        child,
                        context,
                        state,
                        depth + 1,
                    )?);
                }
            }
            Rewrite::Exclusion { base, subtract } => {
                node.children.push(self.expand_rewrite(
                    relation,
                    object,
                    base,
                    context,
                    state,
                    depth + 1,
                )?);
                node.children.push(self.expand_rewrite(
                    relation,
                    object,
                    subtract,
                    context,
                    state,
                    depth + 1,
                )?);
            }
            Rewrite::Condition { .. } => {}
        }
        node.visited = state.visited;
        Ok(node)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct EvalKey {
    subject: SubjectRef,
    relation: Relation,
    object: ObjectRef,
}

impl EvalKey {
    fn new(subject: &SubjectRef, relation: &Relation, object: &ObjectRef) -> Self {
        Self {
            subject: subject.clone(),
            relation: relation.clone(),
            object: object.clone(),
        }
    }
}

#[derive(Default)]
struct EvaluationState {
    visited: usize,
    stack: BTreeSet<EvalKey>,
    memo: BTreeMap<EvalKey, bool>,
}

fn condition_matches(name: &str, context: &AuthzContext) -> bool {
    if let Some((key, expected)) = name.split_once('=') {
        return context
            .attributes
            .get(key)
            .or_else(|| context.token_claims.get(key))
            .map(|actual| actual == expected)
            .unwrap_or(false);
    }

    context
        .attributes
        .get(name)
        .or_else(|| context.token_claims.get(name))
        .map(|actual| matches!(actual.as_str(), "true" | "1" | "yes"))
        .unwrap_or(false)
}

fn rewrite_name(rewrite: &Rewrite) -> &'static str {
    match rewrite {
        Rewrite::Direct => "direct",
        Rewrite::ComputedUserset { .. } => "computed_userset",
        Rewrite::TupleToUserset { .. } => "tuple_to_userset",
        Rewrite::Union { .. } => "union",
        Rewrite::Intersection { .. } => "intersection",
        Rewrite::Exclusion { .. } => "exclusion",
        Rewrite::Condition { .. } => "condition",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ObjectType, RelationDefinition};

    fn relation(value: &str) -> Relation {
        Relation::new(value).unwrap()
    }

    fn subject(value: &str) -> SubjectRef {
        SubjectRef::new(value).unwrap()
    }

    fn object(value: &str) -> ObjectRef {
        ObjectRef::new(value).unwrap()
    }

    fn tenant(value: &str) -> TenantRef {
        TenantRef::new(value).unwrap()
    }

    fn project_model(viewer: RelationDefinition) -> AuthorizationModel {
        AuthorizationModel::new("model_1")
            .with_type(ObjectType::new("project").with_relation(relation("viewer"), viewer))
    }

    #[test]
    fn check_allows_direct_matching_tuple() {
        let viewer = relation("viewer");
        let model = project_model(RelationDefinition::direct());
        let subject = subject("user:alice");
        let object = object("project:demo");
        let tuples = vec![RelationshipTuple::new(
            subject.clone(),
            viewer.clone(),
            object.clone(),
        )];
        let evaluator = Evaluator::new(model, tuples);

        let decision = evaluator
            .check(&subject, &viewer, &object, &AuthzContext::default())
            .unwrap();

        assert!(decision.allowed);
    }

    #[test]
    fn check_denies_when_tuple_is_missing() {
        let viewer = relation("viewer");
        let model = project_model(RelationDefinition::direct());
        let evaluator = Evaluator::new(model, Vec::new());

        let decision = evaluator
            .check(
                &subject("user:alice"),
                &viewer,
                &object("project:demo"),
                &AuthzContext::default(),
            )
            .unwrap();

        assert!(!decision.allowed);
    }

    #[test]
    fn computed_userset_reuses_another_relation_on_same_object() {
        let owner = relation("owner");
        let viewer = relation("viewer");
        let model = AuthorizationModel::new("model_1").with_type(
            ObjectType::new("project")
                .with_relation(owner.clone(), RelationDefinition::direct())
                .with_relation(
                    viewer.clone(),
                    RelationDefinition::computed_userset(owner.clone()),
                ),
        );
        let user = subject("user:alice");
        let project = object("project:demo");
        let tuples = vec![RelationshipTuple::new(user.clone(), owner, project.clone())];

        let decision = Evaluator::new(model, tuples)
            .check(&user, &viewer, &project, &AuthzContext::default())
            .unwrap();

        assert!(decision.allowed);
    }

    #[test]
    fn tuple_to_userset_inherits_from_parent_object() {
        let parent = relation("parent");
        let viewer = relation("viewer");
        let model = AuthorizationModel::new("model_1")
            .with_type(
                ObjectType::new("folder")
                    .with_relation(viewer.clone(), RelationDefinition::direct()),
            )
            .with_type(
                ObjectType::new("project")
                    .with_relation(parent.clone(), RelationDefinition::direct())
                    .with_relation(
                        viewer.clone(),
                        RelationDefinition::tuple_to_userset(parent.clone(), viewer.clone()),
                    ),
            );
        let user = subject("user:alice");
        let folder = object("folder:root");
        let project = object("project:demo");
        let tuples = vec![
            RelationshipTuple::new(user.clone(), viewer.clone(), folder),
            RelationshipTuple::new(
                SubjectRef::new("folder:root").unwrap(),
                parent,
                project.clone(),
            ),
        ];

        let decision = Evaluator::new(model, tuples)
            .check(&user, &viewer, &project, &AuthzContext::default())
            .unwrap();

        assert!(decision.allowed);
    }

    #[test]
    fn union_intersection_and_exclusion_are_evaluated() {
        let viewer = relation("viewer");
        let member = relation("member");
        let approved = relation("approved");
        let blocked = relation("blocked");
        let model = AuthorizationModel::new("model_1").with_type(
            ObjectType::new("project")
                .with_relation(member.clone(), RelationDefinition::direct())
                .with_relation(approved.clone(), RelationDefinition::direct())
                .with_relation(blocked.clone(), RelationDefinition::direct())
                .with_relation(
                    viewer.clone(),
                    RelationDefinition::union(vec![
                        Rewrite::intersection(vec![
                            Rewrite::computed_userset(member.clone()),
                            Rewrite::computed_userset(approved.clone()),
                        ]),
                        Rewrite::exclusion(
                            Rewrite::computed_userset(member.clone()),
                            Rewrite::computed_userset(blocked.clone()),
                        ),
                    ]),
                ),
        );
        let user = subject("user:alice");
        let project = object("project:demo");
        let tuples = vec![
            RelationshipTuple::new(user.clone(), member, project.clone()),
            RelationshipTuple::new(user.clone(), approved, project.clone()),
        ];

        let decision = Evaluator::new(model, tuples)
            .check(&user, &viewer, &project, &AuthzContext::default())
            .unwrap();

        assert!(decision.allowed);
    }

    #[test]
    fn contextual_tuples_and_conditions_allow_abac_style_checks() {
        let viewer = relation("viewer");
        let model = project_model(RelationDefinition::direct());
        let user = subject("user:alice");
        let project = object("project:demo");
        let context = AuthzContext {
            attributes: BTreeMap::from([("business_hours".to_string(), "true".to_string())]),
            contextual_tuples: vec![RelationshipTuple::new(
                user.clone(),
                viewer.clone(),
                project.clone(),
            )
            .with_condition("business_hours")],
            ..AuthzContext::default()
        };

        let decision = Evaluator::new(model, Vec::new())
            .check(&user, &viewer, &project, &context)
            .unwrap();

        assert!(decision.allowed);
    }

    #[test]
    fn equality_conditions_can_read_token_claims() {
        let viewer = relation("viewer");
        let model = project_model(RelationDefinition::condition("plan=enterprise"));
        let context = AuthzContext {
            token_claims: BTreeMap::from([("plan".to_string(), "enterprise".to_string())]),
            ..AuthzContext::default()
        };

        let decision = Evaluator::new(model, Vec::new())
            .check(
                &subject("user:alice"),
                &viewer,
                &object("project:demo"),
                &context,
            )
            .unwrap();

        assert!(decision.allowed);
    }

    #[test]
    fn tenant_context_filters_tenant_scoped_tuples() {
        let viewer = relation("viewer");
        let model = project_model(RelationDefinition::direct());
        let user = subject("user:alice");
        let project = object("project:demo");
        let tuples = vec![
            RelationshipTuple::new(user.clone(), viewer.clone(), project.clone())
                .with_tenant(tenant("tenant:other")),
        ];
        let context = AuthzContext {
            tenant_id: Some(tenant("tenant:default")),
            ..AuthzContext::default()
        };

        let decision = Evaluator::new(model, tuples)
            .check(&user, &viewer, &project, &context)
            .unwrap();

        assert!(!decision.allowed);
    }

    #[test]
    fn list_objects_returns_allowed_objects_in_deterministic_order() {
        let viewer = relation("viewer");
        let model = project_model(RelationDefinition::direct());
        let user = subject("user:alice");
        let tuples = vec![
            RelationshipTuple::new(user.clone(), viewer.clone(), object("project:b")),
            RelationshipTuple::new(user.clone(), viewer.clone(), object("project:a")),
            RelationshipTuple::new(subject("user:bob"), viewer.clone(), object("project:c")),
        ];

        let objects = Evaluator::new(model, tuples)
            .list_objects(&user, &viewer, "project", &AuthzContext::default())
            .unwrap();

        assert_eq!(
            objects
                .into_iter()
                .map(|object| object.to_string())
                .collect::<Vec<_>>(),
            vec!["project:a", "project:b"]
        );
    }

    #[test]
    fn expand_returns_direct_subjects() {
        let viewer = relation("viewer");
        let model = project_model(RelationDefinition::direct());
        let project = object("project:demo");
        let tuples = vec![RelationshipTuple::new(
            subject("user:alice"),
            viewer.clone(),
            project.clone(),
        )];

        let expansion = Evaluator::new(model, tuples)
            .expand(&viewer, &project, &AuthzContext::default())
            .unwrap();

        assert_eq!(expansion.rewrite, "direct");
        assert_eq!(expansion.subjects, vec![subject("user:alice")]);
    }

    #[test]
    fn cycles_are_rejected() {
        let viewer = relation("viewer");
        let model = project_model(RelationDefinition::computed_userset(viewer.clone()));

        let error = Evaluator::new(model, Vec::new())
            .check(
                &subject("user:alice"),
                &viewer,
                &object("project:demo"),
                &AuthzContext::default(),
            )
            .unwrap_err();

        assert_eq!(error, AuthzError::CycleDetected);
    }

    #[test]
    fn max_depth_is_enforced() {
        let owner = relation("owner");
        let viewer = relation("viewer");
        let model = AuthorizationModel::new("model_1").with_type(
            ObjectType::new("project")
                .with_relation(owner.clone(), RelationDefinition::direct())
                .with_relation(
                    viewer.clone(),
                    RelationDefinition::computed_userset(owner.clone()),
                ),
        );

        let error = Evaluator::new(model, Vec::new())
            .with_max_depth(0)
            .check(
                &subject("user:alice"),
                &viewer,
                &object("project:demo"),
                &AuthzContext::default(),
            )
            .unwrap_err();

        assert_eq!(error, AuthzError::MaxDepthExceeded);
    }

    #[test]
    fn unknown_model_shape_is_rejected_by_json_parser() {
        let error =
            AuthorizationModel::from_json(r#"{"model_id":"","schema_version":"1.0","types":{}}"#)
                .unwrap_err();

        assert_eq!(error.public_code(), "validation");
    }
}
