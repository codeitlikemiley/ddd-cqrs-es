---
title: ddd-authz PRD
description: Plan the reusable RBAC, ReBAC, and ABAC authorization crate for the Spin-native auth stack.
---

# ddd-authz PRD

## Status

implemented

## Goal

Create a reusable `ddd-authz` crate that provides OpenFGA-like relationship
authorization for Spin/WASI applications without depending on the OpenFGA
server. The crate must support RBAC, ReBAC, and a bounded ABAC layer through one
shared evaluator and one durable relationship tuple model.

## Non-Goals

- Do not attempt wire-level compatibility with every OpenFGA API in v1.
- Do not add a general-purpose policy language before the WASI dependency and
  safety model are proven.
- Do not make authorization checks query arbitrary event payload JSON.
- Do not hide consistency limits. Relationship checks are only as current as the
  tuple store and contextual tuples supplied to the request.

## Success Criteria

- A service can answer `Check`, `BatchCheck`, `ListObjects`, and `Expand` from a
  versioned authorization model and tuple store.
- RBAC is represented as relationships such as user to role and role to
  permission, not as a separate engine.
- ReBAC supports direct relations, computed usersets, tuple-to-userset, union,
  intersection, and exclusion.
- ABAC v1 supports typed request context and token claims through contextual
  tuples and simple conditions.
- Evaluation is bounded by max depth, max node count, cycle detection, and
  request-scoped memoization.

## Interfaces

### Crate and Features

- Crate name: `ddd-authz`.
- Default feature: `std`.
- Required feature gates:
  - `serde`: model, tuple, and DTO serialization.
  - `json`: JSON model format.
  - `wasi`: WASI-safe runtime helpers.
  - `tracing`: structured decision logs.

### Core Types

- `SubjectRef`: `user:123`, `service:billing`, or `team:ops#member`.
- `ObjectRef`: `tenant:acme`, `project:abc`, or `invoice:123`.
- `Relation`: stable string relation name such as `viewer`, `editor`, `owner`,
  `member`, or `can_approve`.
- `RelationshipTuple`: subject, relation, object, optional condition name, and
  optional tenant ID.
- `AuthorizationModel`: versioned object type definitions and relation rewrite
  rules.
- `AuthzContext`: request attributes, token claims, tenant ID, request time, and
  contextual tuples.
- `Decision`: allow or deny with model ID, matched path, visited count, and
  optional diagnostic reason.

### Model Format

The JSON model format must be stable and explicit:

- `model_id`
- `schema_version`
- `types`
- per type: `relations`
- per relation: one of direct relation, computed userset, tuple-to-userset,
  union, intersection, exclusion, or condition reference.

The first implementation should include a small parser and validator. A DSL may
be added later, but JSON is the durable v1 interface for generated templates,
REST, and gRPC.

### Application Service APIs

- `WriteAuthorizationModel`
- `ActivateAuthorizationModel`
- `ReadAuthorizationModel`
- `WriteRelationshipTuples`
- `DeleteRelationshipTuples`
- `ReadRelationshipTuples`
- `Check`
- `BatchCheck`
- `ListObjects`
- `Expand`

All write APIs require idempotency keys. All read/check APIs require an explicit
model ID or `active` model selector. If no active model exists, checks deny with
a typed configuration error.

### REST and gRPC DTO Shape

- REST paths are owned by the Spin app but should use stable route names:
  `/api/authz/check`, `/api/authz/batch-check`, `/api/authz/list-objects`,
  `/api/authz/expand`, `/api/authz/models`, and `/api/authz/tuples`.
- gRPC service name: `authz.v1.AuthzService`.
- gRPC methods: `Check`, `BatchCheck`, `ListObjects`, `Expand`,
  `WriteAuthorizationModel`, `ActivateAuthorizationModel`,
  `WriteRelationshipTuples`, and `DeleteRelationshipTuples`.

## Implementation Milestones

1. Add core types, JSON model parser, validator, and in-memory tuple/model
   stores.
   - Status: done. `AuthorizationModel::from_json`, `validate`, and
     constructor helpers define the durable JSON model surface for direct,
     computed userset, tuple-to-userset, set, exclusion, and condition rewrites.
2. Implement direct relation and computed userset evaluation with bounded graph
   traversal.
   - Status: done. `Evaluator::check` supports direct relations and computed
     usersets with max-depth, max-node, cycle detection, and request-scoped
     memoization.
3. Add tuple-to-userset, union, intersection, and exclusion.
   - Status: done. The evaluator supports inherited object permissions,
     unions, intersections, and exclusions through the shared rewrite tree.
4. Add contextual tuples and simple typed conditions for ABAC v1.
   - Status: done. `AuthzContext` carries tenant ID, request attributes, token
     claims, and contextual tuples. Named boolean and equality conditions are
     evaluated against attributes and token claims.
5. Add list and expand APIs with explicit limits and deterministic ordering.
   - Status: done. `Evaluator::list_objects` returns allowed objects in stable
     order and `Evaluator::expand` returns a deterministic graph node tree.
6. Add storage adapters and projection hooks after the storage PRD lands.
   - Status: done for the Spin auth stack. The runtime writes and activates
     model JSON, persists tenant-scoped relationship tuples, loads the active
     model and tenant tuple set for checks, and exposes storage-backed REST
     check/list/expand plus gRPC check/list/expand/read-model/read-tuples
     methods.

## Verification

- `cargo test -p ddd-authz --all-features`.
- `cargo check -p ddd-authz --target wasm32-wasip2 --no-default-features --features serde,json,wasi`.
- Unit tests for owner grants, role grants, inherited project permissions,
  team membership, tenant isolation, contextual tuples, condition pass/fail,
  cycles, max depth, unknown model, and unknown relation.
- Golden JSON tests for model parsing and validation errors.
- `rtk env BASE_URL=http://127.0.0.1:3008 bash examples/auth-stack/scripts/verify_auth_stack.sh`
  against a live Spin server proves stored model activation, tuple write,
  check, list-objects, and expand behavior through REST.
