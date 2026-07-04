# Auth Stack Template

This template entry is advertised by `ddd capabilities --json` for projects
created with:

```bash
ddd init auth-stack --preset auth-stack --runtime spin --db sqlite --transport both --ui leptos
```

The renderer lives in `crates/ddd-cli/src/render.rs` so generated projects can
derive names, manifests, and feature flags from CLI arguments. Generated
projects include `spin.production.toml.example` as the exact-host production
manifest starting point.
