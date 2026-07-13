---
title: Archived Authentication Roadmaps
description: Historical design documents superseded by the consolidated wasi-auth fullstack architecture.
---

# Archived authentication roadmaps

The documents in this directory describe the former multi-crate
`ddd-auth`/`ddd-authz` and `auth-stack` design. They are retained as historical
implementation evidence, not as the current product contract.

The current source of truth is the
[wasi-auth fullstack guide](../production/wasi-auth-fullstack.md), the canonical
CLI `fullstack` template, and `examples/fullstack-app`, which must remain
byte-for-byte generated from that template.

Current decisions:

- publish one auth crate, `wasi-auth`;
- keep embedded Cedar as the default production authorizer;
- keep direct SpiceDB opt-in until it passes the production latency gate;
- derive all authority from `VerifiedAuthContext`, never admin request fields;
- use native trusted ingress in production;
- expose Leptos islands, REST, and Spin/Tonic gRPC from one HTTP component;
- retain the former linear evaluator only in testkit;
- support PostgreSQL production and Spin SQLite development in the template;
- do not claim stable Spin support until a tagged runtime passes browser,
  final-WASI, all four gRPC modes, soak, and performance gates.

Historical tracker status labels and command output may refer to deleted crates,
raw tuple administration, MySQL template profiles, or shared admin tokens. Do
not copy those patterns into new code or operational runbooks.
