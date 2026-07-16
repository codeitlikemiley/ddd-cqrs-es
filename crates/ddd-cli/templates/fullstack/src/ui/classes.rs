//! Shared Tailwind class groups for UI primitives.
//!
//! Prefer these constants over free-form utility strings so call sites stay
//! readable and visual tokens stay centralized during the pure-Tailwind
//! migration. Append extras via the optional `class` props on components.

/// Merge a base class string with an optional extra class list.
pub fn with_extra(base: &str, extra: Option<&str>) -> String {
    match extra.map(str::trim).filter(|s| !s.is_empty()) {
        Some(extra) => format!("{base} {extra}"),
        None => base.to_owned(),
    }
}

// ── Buttons ────────────────────────────────────────────────────────────────

/// Inverse filled primary action (legacy `.primary-button`).
pub const BTN_PRIMARY: &str = "inline-flex items-center justify-center min-h-[42px] rounded-[10px] px-3.5 py-2.5 text-[13px] font-semibold no-underline cursor-pointer transition-[background-color,border-color,color,transform,opacity] duration-[180ms] ease-in-out bg-inverse text-on-inverse border border-inverse hover:bg-[color-mix(in_srgb,var(--bg-inverse)_82%,var(--text-primary))] active:translate-y-px disabled:cursor-wait disabled:opacity-55";

/// Outlined secondary action (legacy `.secondary-button` / `.link-button`).
pub const BTN_SECONDARY: &str = "inline-flex items-center justify-center min-h-[42px] rounded-[10px] px-3.5 py-2.5 text-[13px] font-semibold no-underline cursor-pointer transition-[background-color,border-color,color,transform,opacity] duration-[180ms] ease-in-out bg-surface text-primary border border-border-strong hover:bg-surface-hover hover:border-focus active:translate-y-px disabled:cursor-wait disabled:opacity-55";

/// Pill auth submit (legacy `.auth-submit`).
pub const BTN_AUTH_SUBMIT: &str = "inline-flex items-center justify-center gap-2.5 min-h-11 rounded-full px-4 text-sm font-semibold cursor-pointer bg-inverse text-on-inverse border border-inverse transition-[background-color,transform,opacity] duration-[180ms] ease-in-out hover:bg-[color-mix(in_srgb,var(--bg-inverse)_82%,var(--text-primary))] active:translate-y-px disabled:cursor-wait disabled:opacity-55";

// ── Surfaces ───────────────────────────────────────────────────────────────

/// Standard card panel (legacy `.panel`).
pub const PANEL: &str = "grid gap-3.5 min-w-0 rounded-[14px] border border-border-subtle bg-surface p-6";

/// Nested / compact card (legacy `.compact-panel`).
pub const PANEL_COMPACT: &str = "rounded-[10px] border border-border-subtle bg-surface-subtle p-3.5 shadow-none";

// ── Fields ─────────────────────────────────────────────────────────────────

/// Vertical field stack (legacy `.auth-fields`).
pub const FIELD_GROUP: &str = "grid gap-4";

/// Labeled field shell (legacy `.auth-field`).
pub const FIELD: &str = "grid gap-2 text-[13px] font-medium text-primary";

/// Hint under a field (legacy `.auth-field small`).
pub const FIELD_HINT: &str = "text-xs leading-normal text-tertiary";

/// Text input chrome (legacy `.auth-input`).
pub const INPUT: &str = "w-full min-h-11 rounded-[10px] border border-border-strong bg-surface px-3 py-2.5 text-primary outline-none placeholder:text-tertiary focus:border-focus focus:shadow-[0_0_0_3px_color-mix(in_srgb,var(--focus-ring)_18%,transparent)]";

// ── Feedback ───────────────────────────────────────────────────────────────

/// Error banner (legacy `.error-banner` / `.auth-error`).
pub const BANNER_ERROR: &str = "m-0 rounded-[10px] border border-[color-mix(in_srgb,var(--danger)_30%,var(--border-subtle))] bg-[color-mix(in_srgb,var(--danger)_8%,var(--bg-surface))] px-3 py-2.5 text-[13px] leading-normal text-danger";

/// Success / notice surface (legacy `.auth-success` / `.auth-notice`).
pub const BANNER_SUCCESS: &str = "m-0 grid gap-2 rounded-[10px] border border-border-subtle bg-surface-subtle px-3 py-2.5 text-[13px] leading-normal text-secondary";

/// Muted result line (legacy `.result-line`).
pub const RESULT_LINE: &str = "m-0 text-[13px] leading-normal text-secondary";

/// Section / kicker label (legacy `.section-label`).
pub const SECTION_LABEL: &str = "m-0 text-xs font-semibold uppercase tracking-[0.08em] text-tertiary";

// ── Auth brand ─────────────────────────────────────────────────────────────

pub const AUTH_BRAND: &str = "mb-10 flex items-center gap-2.5";
pub const AUTH_LOGO: &str = "inline-flex h-9 w-9 flex-none items-center justify-center rounded-[10px] bg-inverse text-sm font-bold text-on-inverse";
pub const AUTH_BRAND_NAME: &str = "m-0 text-sm font-semibold leading-tight tracking-tight";
pub const AUTH_BRAND_META: &str = "mt-0.5 mb-0 text-xs leading-snug text-tertiary";

// ── Shells ─────────────────────────────────────────────────────────────────

/// Auth page full-viewport center (legacy `.auth-page`).
pub const AUTH_PAGE: &str = "relative box-border flex min-h-dvh w-full flex-col items-center justify-center bg-canvas px-4 py-8 text-primary";

/// Auth card (legacy `.auth-card`).
pub const AUTH_CARD: &str = "w-full max-w-[456px] flex-none rounded-[14px] border border-border-subtle bg-surface p-8 shadow-soft";

/// Auth form stack (legacy `.auth-form`).
pub const AUTH_FORM: &str = "grid gap-7";

/// Auth title (legacy `.auth-title`).
pub const AUTH_TITLE: &str = "mt-3 mb-0 text-[32px] font-semibold leading-[1.05] tracking-tight";

/// Auth body copy (legacy `.auth-copy`).
pub const AUTH_COPY: &str = "mt-3 mb-0 max-w-[34ch] text-[15px] leading-relaxed text-secondary";

/// Sign-in / register segmented control shell.
pub const AUTH_MODE_SWITCH: &str = "grid grid-cols-2 gap-1 rounded-[10px] border border-border-subtle bg-surface-subtle p-1";

/// Segmented control button.
pub const AUTH_MODE_BUTTON: &str = "min-h-10 rounded-lg border-0 bg-transparent px-2.5 text-[13px] font-semibold text-secondary cursor-pointer transition-[background-color,color,transform] duration-[180ms] ease-in-out hover:text-primary active:translate-y-px";

/// Active segment.
pub const AUTH_MODE_BUTTON_ACTIVE: &str = "min-h-10 rounded-lg border-0 bg-surface px-2.5 text-[13px] font-semibold text-primary shadow-soft cursor-pointer transition-[background-color,color,transform] duration-[180ms] ease-in-out active:translate-y-px";

/// Secondary text-style auth control.
pub const AUTH_SECONDARY: &str = "inline-flex items-center justify-center min-h-10 rounded-[10px] border border-border-strong bg-surface px-3 text-[13px] font-semibold text-primary cursor-pointer hover:bg-surface-hover disabled:cursor-wait disabled:opacity-55";

/// Inline field error.
pub const AUTH_INLINE_ERROR: &str = "m-0 text-xs leading-normal text-danger";

/// Centered text link under forms.
pub const AUTH_TEXT_LINK: &str = "inline-flex justify-center no-underline text-primary";

/// Divider between credential form and OAuth/passkey.
pub const AUTH_DIVIDER: &str = "flex items-center gap-3 text-xs font-semibold uppercase tracking-wider text-tertiary before:h-px before:flex-1 before:bg-border-subtle after:h-px after:flex-1 after:bg-border-subtle";

/// Alternate method stack.
pub const AUTH_ALT_STACK: &str = "grid gap-2";

/// OAuth / passkey alternate button.
pub const AUTH_ALT_BUTTON: &str = "inline-flex w-full items-center justify-center min-h-11 rounded-[10px] border border-border-strong bg-surface px-3 text-[13px] font-semibold text-primary cursor-pointer hover:bg-surface-hover disabled:cursor-wait disabled:opacity-55";

/// Spinner inside auth submit.
pub const AUTH_BUTTON_SPINNER: &str = "inline-block h-3.5 w-3.5 animate-spin rounded-full border-2 border-on-inverse/30 border-t-on-inverse";

/// Button row / action cluster.
pub const BUTTON_ROW: &str = "flex flex-wrap items-center gap-2";

/// Account settings column (legacy `.account-page`).
pub const ACCOUNT_PAGE: &str = "box-border mx-auto w-full max-w-[640px] min-w-0 pb-12";
pub const ACCOUNT_PAGE_HEADER: &str = "mb-6 border-b border-border-subtle pb-5";
pub const ACCOUNT_PAGE_TITLE: &str = "m-0 text-[clamp(26px,3.2vw,32px)] font-semibold leading-tight tracking-tight";
pub const ACCOUNT_PAGE_SUBTITLE: &str = "mt-2 mb-0 text-sm text-secondary";
pub const ACCOUNT_PAGE_BODY: &str = "grid gap-4 min-w-0";

/// Workspace page header strip.
pub const WORKSPACE_PAGE_HEADER: &str = "mb-7 border-b border-border-subtle px-0 pb-[22px] pt-2";
pub const WORKSPACE_PAGE_TITLE: &str = "m-0 max-w-[20ch] text-[clamp(28px,3.8vw,36px)] font-semibold leading-[1.08] tracking-tight";
pub const WORKSPACE_PAGE_SUBTITLE: &str = "text-sm text-secondary";
pub const PAGE_GRID: &str = "grid gap-4 min-w-0";

/// Error interrupt page.
pub const ERROR_PAGE: &str = "grid min-h-dvh place-items-center bg-canvas p-6";
pub const ERROR_CARD: &str = "w-full max-w-[480px] rounded-[14px] border border-border-subtle bg-surface p-8 shadow-soft";
pub const ERROR_TITLE: &str = "m-0 text-[clamp(30px,6vw,42px)] font-semibold tracking-tight";
pub const ERROR_COPY: &str = "mt-3 mb-0 max-w-[34ch] text-[15px] leading-relaxed text-secondary";
pub const ERROR_ACTIONS: &str = "mt-6 flex flex-wrap gap-2";

// ── Filter combobox ────────────────────────────────────────────────────────

pub const COMBOBOX: &str = "relative min-w-0";
pub const COMBOBOX_LABEL: &str = "grid gap-1.5 text-xs font-semibold text-secondary";
pub const COMBOBOX_CONTROL: &str = "relative";
pub const COMBOBOX_INPUT: &str = "w-full min-h-10 rounded-[10px] border border-border-strong bg-surface py-2 pl-3 pr-9 text-[13px] text-primary outline-none placeholder:text-tertiary focus:border-focus focus:shadow-[0_0_0_3px_color-mix(in_srgb,var(--focus-ring)_18%,transparent)] disabled:cursor-not-allowed disabled:opacity-55";
pub const COMBOBOX_CHEVRON: &str = "pointer-events-none absolute right-3 top-1/2 h-0 w-0 -translate-y-1/2 border-x-4 border-t-[5px] border-x-transparent border-t-secondary";
pub const COMBOBOX_LIST: &str = "absolute z-40 mt-1 max-h-60 w-full overflow-auto rounded-[10px] border border-border-subtle bg-surface p-1 shadow-soft";
pub const COMBOBOX_EMPTY: &str = "px-2.5 py-2 text-[13px] text-tertiary";
pub const COMBOBOX_OPTION: &str = "cursor-pointer rounded-lg px-2.5 py-2 text-[13px] text-primary";
pub const COMBOBOX_OPTION_ACTIVE: &str = "bg-surface-hover";
pub const COMBOBOX_OPTION_SELECTED: &str = "font-semibold";
