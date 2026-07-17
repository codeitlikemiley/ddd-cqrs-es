---
title: Persistent chrome on Leptos islands
description: Keep workspace chrome (nav, account menu, theme) mounted across soft navigations so islands do not re-flash loaders.
---

# Persistent chrome on Leptos islands

This guide documents the **workspace chrome persistence** technique used in
`examples/fullstack-app`: sidebar org switcher, account menu, and theme toggle
stay **the same DOM nodes** when the user moves between dashboard, settings, and
account pages.

Use it whenever you build a product shell on **Leptos islands + Spin/WASI** and
chrome must not look like it is “loading again” on every route change.

## The problem

With pure multi-page islands (no client router script), every `<a href>` is a
full document load. The whole shell re-SSR’s and every `#[island]` remounts.

Even with a client cache:

```rust
let (snapshot, set_snapshot) = signal(None);
// async: after_island_hydration → read cache → set_snapshot(Some(...))
```

each remount still starts from `None`, swaps the view tree, and flickers.

Symptoms:

- Org switcher / account flyout flash skeleton or generic “Workspace” shell
- Theme toggle briefly shows the wrong glyph (default then localStorage)
- Network tab fills with repeated `get_current_session` / `list_organizations`

## Two layers of the fix

| Layer | What it does |
|-------|----------------|
| **A. Cache-first islands** | Session + org list in memory + `sessionStorage`; paint real labels as soon as possible after hydrate |
| **B. Persistent chrome soft-nav** | Do not destroy chrome islands on in-app navigation; only swap page content |

**B is the real fix for flicker.** A is still useful for cold load and for when
islands must remount (hard refresh, leaving the shell).

## Architecture

```text
┌─────────────────────────────────────────────────────────┐
│ #workspace-shell                                        │
│  ┌──────────────┐  ┌──────────────────────────────────┐ │
│  │ aside        │  │ main                             │ │
│  │  brand       │  │  topbar (title region swaps)     │ │
│  │  #workspace- │  │  #workspace-content  ← SWAPPED   │ │
│  │   primary-nav│  │    (page body + page islands)    │ │
│  │   ← SWAPPED  │  │                                  │ │
│  │  #workspace- │  │                                  │ │
│  │   chrome-foot│  │                                  │ │
│  │   org/account│  │                                  │ │
│  │   theme      │  │                                  │ │
│  │   ← PERSIST  │  │                                  │ │
│  └──────────────┘  └──────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

Soft navigation for workspace → workspace:

1. Intercept same-origin clicks (capture phase).
2. `fetch(url, { headers: { "Islands-Router": "true" } })`.
3. Parse HTML; require `#workspace-shell` on both sides.
4. Replace **only**:
   - `#workspace-content`
   - `#workspace-primary-nav`
   - `#workspace-topbar-title`
5. Hydrate **new** `leptos-island` nodes in those regions (`window.__hydrateIsland`).
6. `history.pushState` / `replaceState` + fire nav-active event.

Chrome foot islands are **never** touched, so their Leptos state stays alive.

## Prerequisites

### 1. Enable islands router on the shell

```rust
// src/app/router.rs — document shell
<HydrationScripts
    options=options.clone()
    islands=true
    islands_router=true   // injects islands_routing.js + soft-nav protocol
    root=""
/>
```

`islands=true` alone is **not** enough. Without `islands_router=true`, every hop
is a full document load.

### 2. Server must honor `Islands-Router`

With `leptos-wasi-runtime` / `leptos_wasi` feature `islands-router`, the handler
detects the header, provides `IslandsRouterNavigation`, and can omit redundant
hydration scripts on subsequent navigations.

The fullstack example already enables:

```toml
leptos_wasi = { ..., features = ["wasip3", "islands-router", "tracing"] }
```

### 3. Stable region IDs in the shell markup

| Region | Role |
|--------|------|
| `#workspace-shell` | Detect “still in product chrome” |
| `#workspace-content` | Page body (always swap) |
| `#workspace-primary-nav` | Product ↔ settings links (swap) |
| `#workspace-topbar-title` | Page title text (swap) |
| `#workspace-chrome-foot` | Org / account / theme (persist) |

Mark persistent areas with `data-chrome-persist="true"` for documentation and
future tooling.

Optional CSS for smoother transitions:

```css
/* via Tailwind arbitrary properties on classes */
[view-transition-name:workspace-chrome-foot]
[view-transition-name:workspace-content]
```

## Implementation sketch (client)

Reference: `examples/fullstack-app/src/app/mod.rs` → `initWorkspaceChromePersist`
(inline JS exported to WASM via `#[wasm_bindgen(inline_js = ...)]`).

Installed once from a small shell island:

```rust
#[island]
pub fn WorkspaceSidebarControls() -> impl IntoView {
    Effect::new(move |_| {
        #[cfg(feature = "hydrate")]
        {
            init_workspace_sidebar();
            init_workspace_chrome_persist();
        }
    });
    view! { <span class="hidden" aria-hidden="true"></span> }
}
```

Core navigation rules:

- Only intercept when **both** current and target path are “workspace chrome”
  paths (dashboard, account, organizations, `/org/…`, admin, …).
- Leave login/register/public pages to full navigation.
- Use `stopImmediatePropagation` in the **capture** phase so the default
  islands-router full-tree diff does not remount chrome when branch markers
  diverge.
- On failure (network, missing shell), fall back to `window.location.href`.

Hydrate only unhydrated islands after the swap:

```js
for (const island of root.querySelectorAll("leptos-island")) {
  if (!island.$$hydrated && window.__hydrateIsland) {
    window.__hydrateIsland(island, island.dataset.component);
    island.$$hydrated = true;
  }
}
```

## Cache-first chrome data (layer A)

Shared snapshot for session + memberships (used by org switcher, account menu,
settings nav):

- In-memory `thread_local` (WASM)
- `sessionStorage` key (e.g. `workspace-chrome-v1`) for remounts within the tab
- Coalesced refresh so three islands do not stampede `/api/ui/*`

Pattern:

1. SSR: stable non-pulse shell (or better: SSR real labels from the cookie).
2. Hydrate: if cache hit → `set_snapshot(Ok(cached))` immediately.
3. Background: `refresh_workspace_chrome_cache()` → update if changed.
4. Client active state: `data-nav` + `mark_active_nav(pathname)` (no Router in
   island context).

See `WorkspaceOrgSwitcher`, `WorkspaceUserMenu`, and `WorkspaceSettingsNavLinks`
in `examples/fullstack-app/src/app/workspace/mod.rs`.

## Client-side focus for flyouts

Islands do not sit in the Router context. Highlight the current account /
settings item with DOM attributes:

```html
<a href="/account/profile" data-nav="account-profile" class="...">Profile</a>
```

```js
// mark_active_nav(pathname) toggles .is-active on [data-nav=...]
```

Re-run after soft-nav via `workspace-nav-mark` (dispatched from `pushState`
hooks and chrome-persist).

## What not to do

| Anti-pattern | Why it fails |
|--------------|--------------|
| `browser_load` + skeleton in chrome islands only | Remount → `None` → flash every hop |
| Whole-sidebar `#[island]` | Hydration panics / stuck loaders in practice |
| Rely only on islands-router tree walk | Branch markers in outlet/nav can still replace large ranges |
| Put chrome islands inside `{move \|\| if path …}` branches | Diff replaces the branch including the islands |
| Forget `islands_router=true` | Full reloads forever |

## Verification checklist

After changing chrome:

1. Hard refresh on `/dashboard` (cold hydrate).
2. Tag islands in DevTools (`dataset.probe = "1"`).
3. Click Organizations, Profile, a settings section.
4. Assert **same** `leptos-island` nodes for theme / account / org (probe still
   set).
5. Assert `#workspace-content` text changed.
6. Assert zero `animate-pulse` under `#workspace-chrome-foot`.
7. Confirm console has no hydrate errors.

Playwright sketch:

```js
themeIsland.dataset.persistProbe = "theme-v1";
await page.click('a[href="/organizations"]');
// expect themeIsland.dataset.persistProbe === "theme-v1"
// expect location.pathname === "/organizations"
```

## Relation to the fullstack template

Product files dual-sync to the CLI template:

```bash
# from monorepo root
bash examples/fullstack-app/scripts/sync_fullstack_template.sh
# or check drift
bash examples/fullstack-app/scripts/sync_fullstack_template.sh check
```

Generated apps from `ddd init --preset fullstack` pick up the same shell
regions and `initWorkspaceChromePersist` when the CLI package is released.

## When this technique is enough

- Authenticated product shell with a stable sidebar
- Many routes, same chrome, heavy page islands
- Spin/WASI + islands (not full CSR SPA)

Prefer a full SPA shell only if you need client-only nested routers without SSR
HTML swaps. For most SaaS dashboards on islands, **content-only soft-nav +
cache-first chrome data** is the right balance.

## Reference files

| File | Role |
|------|------|
| `examples/fullstack-app/src/app/mod.rs` | `initWorkspaceChromePersist`, nav-active helpers |
| `examples/fullstack-app/src/app/router.rs` | `HydrationScripts { islands_router: true }` |
| `examples/fullstack-app/src/app/workspace/mod.rs` | Shell regions, chrome islands, cache |
| `examples/fullstack-app/src/app/helpers.rs` | `mark_active_nav` including account flyout |
| `examples/fullstack-app/src/ui/classes.rs` | View-transition names / foot styles |
| `examples/fullstack-app/DESIGN.md` | Product UI tokens + pointers |

## See also

- [Reactive Leptos UI](./leptos-ssr-ui.md) — optimistic UI patterns for counter-style apps
- [WASI auth fullstack](../production/wasi-auth-fullstack.md) — Spin + wasi-auth stack
- Leptos book: [Islands](https://book.leptos.dev/islands.html)
