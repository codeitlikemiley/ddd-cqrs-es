# Fullstack visual system — ChatGPT-inspired neutral

**Audience:** product UI, Tailwind migration, and theme work.  
**Implementation:** `input.css` (tokens + base) and `src/ui/classes.rs` (utilities).  
**Theme control:** `src/app/theme.rs` + `html[data-theme="light|dark|system"]`.

This brief is the source of truth for surface hierarchy. Prefer **semantic tokens**
(`bg-canvas`, `bg-surface`, …) over raw greys so light/dark and future accent work
stay one place.

---

## 1. Principles

1. **Content-first** — chrome is quiet; primary content and forms lead.
2. **Semantic surfaces** — name roles (canvas, sidebar, surface), not paint chips.
3. **One pure-white layer (light)** — cards, inputs, and modals may be pure white;
   the page field must not.
4. **Subtle contrast over decoration** — elevation comes from stacked neutrals,
   soft borders, and light shadows—not heavy chrome.
5. **Accent is late-bound** — neutrals are neutral; brand accent can land later
   without rewriting every component.
6. **Theme is explicit** — light / dark / system via `data-theme`; utilities use
   the `dark:` variant wired in `input.css`.

---

## 2. Light elevation (zinc-25 pattern)

Tailwind greys jump **white → 50**. **50** is already dingy as a full-bleed page
background; **pure white as canvas** is stark—cards disappear into the page.

Insert a **−25** step (Porzio / ChatGPT “super-slightly-off-white”):

| Role | Semantic token | Target | Feel |
|------|----------------|--------|------|
| Page / main field | `--bg-canvas` → `bg-canvas` | ≈ **zinc-25** `oklch(99.2% 0 0)` | Light and airy |
| Shell rail | `--bg-sidebar` → `bg-sidebar` | ≈ **zinc-50** `oklch(98.5% 0 0)` | Quiet frame |
| Cards, inputs, modals | `--bg-surface` / `--bg-elevated` | **pure white** `#ffffff` | Crisp, elevated |
| Inset wells only | `--bg-surface-subtle` | ~`oklch(97.5% 0 0)` | Nested chips, segmented controls |

### Stack (light)

```text
┌─────────────────────────────────────────────┐
│  canvas (zinc-25)                           │
│  ┌──────────┐  ┌──────────────────────────┐ │
│  │ sidebar  │  │  surface (white card)    │ │
│  │ (zinc-50)│  │  inputs / tiles / modals │ │
│  └──────────┘  └──────────────────────────┘ │
└─────────────────────────────────────────────┘
```

### Rules (light)

- **Do** put large regions on `bg-canvas`.
- **Do** put floating UI on `bg-surface` / `bg-elevated`.
- **Do** use `bg-sidebar` only for the rail (or match canvas if a product wants a flat field).
- **Don’t** paint the whole app `bg-white` or raw `zinc-50` — flat or dingy.
- **Don’t** use `surface-subtle` as a full-bleed page background.
- Optional utility: `--color-zinc-25` / `bg-zinc-25` for one-offs; **prefer `bg-canvas`** in product UI.

### Reference values (light)

Canonical values live in `input.css` under `:root` / `html[data-theme="light"]`:

| Token | Value |
|-------|--------|
| `--bg-canvas` | `oklch(99.2% 0 0)` |
| `--bg-sidebar` | `oklch(98.5% 0 0)` |
| `--bg-surface` | `#ffffff` |
| `--bg-elevated` | `#ffffff` |
| `--bg-surface-subtle` | `oklch(97.5% 0 0)` |
| `--bg-surface-hover` | `oklch(96% 0 0)` |
| `--bg-surface-active` | `oklch(94.5% 0 0)` |
| `--border-subtle` | `oklch(92% 0 0)` |
| `--border-strong` | `oklch(86% 0 0)` |
| `--code-bg` | `oklch(98% 0 0)` |
| `--shadow-soft` | `0 8px 24px rgba(0, 0, 0, 0.08)` |

---

## 3. Dark elevation

Same roles, inverted values (already in `input.css`):

| Role | Dark intent |
|------|-------------|
| Canvas | Mid room (`#212121`) — not pure black |
| Sidebar | Slightly deeper chrome (`#171717`) |
| Surface / elevated | Raised panel (`#2f2f2f`) |
| Surface-subtle | Inset, not full-bleed |

Do **not** force light zinc-25 into dark. Dark contrast is “lighter panel on darker room,” not off-white.

---

## 4. Semantic token map

Bridged in `@theme inline` so Tailwind utilities stay aligned with CSS variables:

| Utility prefix | CSS variable | Typical use |
|----------------|--------------|-------------|
| `bg-canvas` | `--bg-canvas` | `html`/`body`, shell, auth page, main field |
| `bg-sidebar` | `--bg-sidebar` | Workspace / settings rail |
| `bg-surface` | `--bg-surface` | Cards, panels, inputs, board tiles |
| `bg-elevated` | `--bg-elevated` | Popovers, sticky chips, “above surface” |
| `bg-surface-subtle` | `--bg-surface-subtle` | Inset wells, inactive segmented segments |
| `bg-surface-hover` / `active` | hover/active | Interactive rows and buttons |
| `text-primary` … `tertiary` | text tokens | Hierarchy |
| `border-subtle` / `strong` | border tokens | Dividers and control edges |
| `bg-accent` / `text-danger` … | status tokens | Actions and feedback |

Implementation constants: `src/ui/classes.rs` (e.g. `AUTH_PAGE` = canvas, `AUTH_CARD` = surface).

---

## 5. Component layering checklist

| Surface | Token |
|---------|--------|
| App shell / main column | `bg-canvas` |
| Sidebar | `bg-sidebar` |
| Auth page backdrop | `bg-canvas` |
| Auth card, account panels, settings cards | `bg-surface` |
| Board tiles, modals, dialogs | `bg-surface` (+ soft shadow where needed) |
| Inputs / textareas / secondary buttons | `bg-surface` |
| Segmented controls, code wells, empty inset regions | `bg-surface-subtle` |
| MFA QR plate | pure white (`bg-white`) — **scan contrast**, intentional exception |

---

## 6. Anti-patterns

| Avoid | Why |
|-------|-----|
| Full-bleed `bg-white` / pure white canvas in light | Stark; cards don’t float (no hierarchy) |
| Full-bleed `zinc-50` / heavy grey page | Dingy; “no contrast” next to white controls |
| Hard-coded `#fff` / Tailwind grey ramps in markup | Breaks dark mode and future token retunes |
| Using `surface-subtle` as the app background | Too heavy; reserved for nested UI |
| Painting modals with `--bg-inverse` as scrim | In dark mode becomes a milky fog — use overlay scrim tokens |

---

## 7. Theme modes

| `data-theme` | Behavior |
|--------------|----------|
| `light` | Light token block |
| `dark` | Dark token block |
| `system` (or unset) | Follows `prefers-color-scheme` |

Toggle lives in the workspace shell foot (`ThemeToggle`). FOUC script in the
document head seeds `data-theme` from `localStorage` (`app-theme`).

---

## 8. Loading states (skeletons)

Prefer **skeleton placeholders** over plain `"Loading members…"` text for async
page/section content. Implementation: `src/ui/skeleton.rs` + `SKEL_*` in
`classes.rs`.

### Rules

1. **Semantic grey only** — `bg-surface-subtle` + `animate-pulse` (via `SKEL_BONE*`).
   No hard-coded `#e8e8ed` / zinc ramps for new loaders.
2. **Layout-preserving** — mirror title, panel, table, or form shape so content
   does not jump when data arrives.
3. **Accessible** — root `aria-busy="true"` and a specific `aria-label`
   (e.g. `"Loading members"`); bones `aria-hidden`.
4. **Compose** — use primitives (`SkeletonBone`, `SkeletonRow`, `SkeletonStack`,
   `SkeletonCircle`, `SkeletonText`, `SkeletonPanel`) for custom layouts; use
   recipes (`FormSkeleton`, `TableSkeleton`, `ListSkeleton`,
   `SettingsPageSkeleton`, `CardGridSkeleton`, `PageHeaderSkeleton`) when they fit.
5. **Not for buttons** — keep control pending copy (`"Saving…"`, `"Loading demos…"`)
   on CTAs; skeletons are for section/page body loads.

### Example

```rust
use crate::ui::{SettingsPageSkeleton, SettingsSkeletonVariant, TableSkeleton};

// Settings form body
view! {
  <Show when=move || data.get().is_none()>
    <SettingsPageSkeleton label="Loading workspace" variant=SettingsSkeletonVariant::Form />
  </Show>
}

// Custom members toolbar + table
view! {
  <TableSkeleton rows=6 cols=3 with_avatar=true label="Loading members" />
}
```

---

## 9. Change process

1. Edit tokens in `examples/fullstack-app/input.css` (this brief stays in sync).
2. Dual-sync product files to the CLI template when required  
   (`scripts/sync_fullstack_template.sh` — `input.css` is on the allowlist).
3. Prefer class constants in `src/ui/classes.rs` over ad-hoc utilities in islands.
4. Rebuild Tailwind via the normal Spin/Cargo frontend pipeline; verify light **and** dark.

When this document and `input.css` disagree, **update both** in the same change.

---

## 10. Credits / inspiration

- ChatGPT-style neutral product chrome (content-first, soft fields).
- [Caleb Porzio — zinc-25 Tailwind note](https://x.com/calebporzio/status/2077409452571934940):
  add a −25 grey so the page field is airy while pure-white cards still contrast.
- Industry baseline for overlays: dark scrim (not inverse-as-fog), ~40–60% black.
