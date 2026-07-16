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
pub const PANEL: &str =
    "grid gap-3.5 min-w-0 rounded-[14px] border border-border-subtle bg-surface p-6";

/// Nested / compact card (legacy `.compact-panel`).
pub const PANEL_COMPACT: &str =
    "rounded-[10px] border border-border-subtle bg-surface-subtle p-3.5 shadow-none";

/// Block panel (legacy `.panel-inline`) — no CSS grid.
pub const PANEL_INLINE: &str =
    "block min-w-0 rounded-[14px] border border-border-subtle bg-surface p-6";

/// Panel heading (legacy `.panel h2`).
pub const PANEL_TITLE: &str = "m-0 text-lg font-semibold tracking-tight";

// ── Fields ─────────────────────────────────────────────────────────────────

/// Vertical field stack (legacy `.auth-fields`).
pub const FIELD_GROUP: &str = "grid gap-4";

/// Labeled field shell (legacy `.auth-field`).
pub const FIELD: &str = "grid gap-2 text-[13px] font-medium text-primary";

/// Hint under a field (legacy `.auth-field small`).
pub const FIELD_HINT: &str = "text-xs leading-normal text-tertiary";

/// Text input chrome (legacy `.auth-input`).
pub const INPUT: &str = "w-full min-h-11 rounded-[10px] border border-border-strong bg-surface px-3 py-2.5 text-primary outline-none placeholder:text-tertiary focus:border-focus focus:shadow-[0_0_0_3px_color-mix(in_srgb,var(--focus-ring)_18%,transparent)]";

/// Multi-line field (legacy `.panel textarea`).
pub const TEXTAREA: &str = "w-full min-h-[120px] resize-y rounded-[10px] border border-border-strong bg-surface px-3 py-2.5 text-primary outline-none placeholder:text-tertiary focus:border-focus focus:shadow-[0_0_0_3px_color-mix(in_srgb,var(--focus-ring)_18%,transparent)]";

// ── Feedback ───────────────────────────────────────────────────────────────

/// Error banner (legacy `.error-banner` / `.auth-error`).
pub const BANNER_ERROR: &str = "m-0 rounded-[10px] border border-[color-mix(in_srgb,var(--danger)_30%,var(--border-subtle))] bg-[color-mix(in_srgb,var(--danger)_8%,var(--bg-surface))] px-3 py-2.5 text-[13px] leading-normal text-danger";

/// Success / notice surface (legacy `.auth-success` / `.auth-notice`).
pub const BANNER_SUCCESS: &str = "m-0 grid gap-2 rounded-[10px] border border-border-subtle bg-surface-subtle px-3 py-2.5 text-[13px] leading-normal text-secondary";

/// Muted result line (legacy `.result-line`).
pub const RESULT_LINE: &str = "m-0 text-[13px] leading-normal text-secondary";

/// Section / kicker label (legacy `.section-label`).
pub const SECTION_LABEL: &str =
    "m-0 text-xs font-semibold uppercase tracking-[0.08em] text-tertiary";

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
pub const AUTH_MODE_SWITCH: &str =
    "grid grid-cols-2 gap-1 rounded-[10px] border border-border-subtle bg-surface-subtle p-1";

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
pub const ACCOUNT_PAGE_TITLE: &str =
    "m-0 text-[clamp(26px,3.2vw,32px)] font-semibold leading-tight tracking-tight";
pub const ACCOUNT_PAGE_SUBTITLE: &str = "mt-2 mb-0 text-sm text-secondary";
pub const ACCOUNT_PAGE_BODY: &str = "grid gap-5 min-w-0";

/// Account card density (legacy `.account-page .panel` padding/gap).
pub const ACCOUNT_PANEL: &str = "grid w-full min-w-0 gap-5 rounded-[14px] border border-border-subtle bg-surface px-7 pb-8 pt-7";

/// Panel title inside account cards.
pub const ACCOUNT_PANEL_TITLE: &str = "m-0 mt-1.5 text-lg font-semibold tracking-tight";

/// Shared lede under account section titles (legacy `.passkey-lede` / `.mfa-lede`).
pub const ACCOUNT_LEDE: &str = "m-0 mt-2.5 max-w-[52ch] text-sm leading-normal text-secondary";

/// Lede without top margin when it follows the panel head directly.
pub const ACCOUNT_LEDE_FLUSH: &str = "m-0 max-w-none text-sm leading-normal text-secondary";

/// Panel head row (legacy `.session-panel-head`).
pub const ACCOUNT_PANEL_HEAD: &str = "m-0 flex items-start justify-between gap-3 p-0";

/// Footer action stack under form fields (legacy `.account-card-actions`).
pub const ACCOUNT_CARD_ACTIONS: &str =
    "mt-1 grid items-start gap-3.5 border-t border-border-subtle bg-transparent pt-5";

/// Monospace value (legacy `.mono-value`). Safe to use outside account too.
pub const MONO_VALUE: &str = "font-mono text-xs tracking-tight";

/// Key/value definition list (legacy `.kv`) + dt/dd parts.
pub const KV_LIST: &str =
    "m-0 grid grid-cols-[minmax(100px,max-content)_minmax(0,1fr)] gap-x-4 gap-y-2.5";
pub const KV_DT: &str = "text-[13px] leading-normal text-secondary";
pub const KV_DD: &str = "m-0 min-w-0 break-words text-[13px] text-primary";

/// Tertiary muted helper (legacy `.board-muted`). For secondary body hints use `RESULT_LINE`.
pub const MUTED: &str = "m-0 text-[13px] text-tertiary";

/// Checkbox / switch row (legacy `.inline-field`).
pub const INLINE_FIELD: &str = "!flex items-center gap-2 [&_input]:min-h-0 [&_input]:w-auto";

/// Screen-reader only (legacy `.sr-only`).
pub const SR_ONLY: &str = "absolute m-[-1px] h-px w-px overflow-hidden whitespace-nowrap border-0 p-0 [clip:rect(0,0,0,0)]";

/// Text-style link (legacy `.text-link`).
pub const TEXT_LINK: &str = "cursor-pointer border-0 bg-transparent p-0 text-left text-[13px] font-semibold text-primary underline-offset-4 hover:text-secondary";

/// Soft success banner (legacy `.success-banner`).
pub const BANNER_OK: &str = "m-0 rounded-[10px] border border-[color-mix(in_srgb,var(--success)_28%,var(--border-subtle))] bg-[color-mix(in_srgb,var(--success)_10%,var(--bg-surface))] px-3 py-2.5 text-[13px] leading-normal text-[color-mix(in_srgb,var(--success)_80%,var(--text-primary))]";

/// Status pill (legacy `.mfa-badge`).
pub const BADGE: &str =
    "flex-none whitespace-nowrap rounded-full px-2.5 py-1.5 text-xs font-semibold tracking-wide";
pub const BADGE_ON: &str = "border border-[color-mix(in_srgb,var(--success)_32%,var(--border-subtle))] bg-[color-mix(in_srgb,var(--success)_14%,var(--bg-surface))] text-primary";
pub const BADGE_OFF: &str = "border border-border-subtle bg-surface-subtle text-secondary";

/// Wizard step chrome shared by MFA + passkeys.
pub const WIZARD_PROGRESS: &str = "mb-[18px] flex items-center gap-2";
pub const WIZARD_STEP: &str = "inline-flex h-6 w-6 items-center justify-center rounded-full border border-border-subtle bg-surface-subtle text-[11px] font-bold text-tertiary";
pub const WIZARD_STEP_ACTIVE: &str = "inline-flex h-6 w-6 items-center justify-center rounded-full border border-inverse bg-inverse text-[11px] font-bold text-on-inverse";
pub const WIZARD_LINE: &str = "h-px max-w-12 flex-auto bg-border-subtle";
pub const WIZARD_LINE_DONE: &str = "h-px max-w-12 flex-auto bg-primary";

/// Numbered setup steps list.
pub const STEPS_PREVIEW: &str = "my-[18px] mb-[22px] grid list-decimal gap-2.5 pl-5 text-sm leading-normal text-secondary marker:text-secondary";
pub const STEPS_PREVIEW_STRONG: &str = "font-semibold text-primary";

/// Status head with badge (legacy `.mfa-status-head`).
pub const STATUS_HEAD: &str =
    "flex flex-row items-start justify-between gap-4 max-[900px]:flex-col";

/// Session list + card chrome.
pub const SESSION_LIST: &str = "grid gap-3.5 p-0 pb-1 m-0";
pub const SESSION_CARD: &str = "grid gap-2.5";
/// Extra for the current-session card only (compose with `SESSION_CARD`).
pub const SESSION_CARD_CURRENT: &str = "!border-border-strong";
pub const SESSION_CARD_HEAD: &str = "flex items-center justify-between gap-2.5";
pub const SESSION_ASSURANCE: &str = "whitespace-nowrap rounded-full border border-border-subtle bg-surface-subtle px-2.5 py-1.5 font-mono text-[11px] font-semibold tracking-wide text-secondary";

/// Client-data slot (display:contents host for async islands).
pub const CLIENT_DATA_SLOT: &str = "contents";

/// Prefixed URL control (legacy `.slug-input-group` / `.onboarding-slug-row`).
pub const SLUG_INPUT_GROUP: &str = "flex w-full min-h-11 items-stretch overflow-hidden rounded-[10px] border border-border-strong bg-surface focus-within:border-focus focus-within:shadow-[0_0_0_3px_color-mix(in_srgb,var(--focus-ring)_18%,transparent)]";
pub const SLUG_INPUT_PREFIX: &str = "inline-flex flex-none items-center whitespace-nowrap border-0 border-r border-border-subtle bg-[var(--bg-muted,#f4f4f5)] px-3 font-mono text-[13px] leading-none text-secondary";
/// Compose with `INPUT` + `MONO_VALUE` via `with_extra` / format.
pub const SLUG_INPUT_FIELD: &str =
    "!min-h-0 !w-auto min-w-0 flex-auto !rounded-none !border-0 !shadow-none focus:!border-0 focus:!shadow-none";

// ── MFA ────────────────────────────────────────────────────────────────────

pub const MFA_FLOW: &str = "grid min-w-0 w-full gap-5";
pub const MFA_OVERVIEW: &str = "grid min-w-0 gap-4";
pub const MFA_FOCUS_WRAP: &str = "grid min-w-0";
pub const MFA_FOCUS_PANEL: &str = "grid w-full min-w-0 max-w-[720px] gap-[18px] rounded-[14px] border border-border-subtle bg-surface px-7 pb-8 pt-7 shadow-soft";
pub const MFA_LEDE_WARN: &str = "m-0 mt-2.5 max-w-[52ch] text-sm leading-normal text-primary";
pub const MFA_HINT: &str = "m-0 mt-2 text-xs leading-normal text-tertiary";
pub const MFA_COPY_FEEDBACK: &str = "m-0 mt-2 text-xs leading-normal text-success";
pub const MFA_STATUS_KV: &str =
    "m-0 mt-[18px] grid grid-cols-[minmax(100px,max-content)_minmax(0,1fr)] gap-x-4 gap-y-2.5";
pub const MFA_ENROLL_GRID: &str =
    "mt-5 grid grid-cols-1 gap-6 min-[901px]:grid-cols-[minmax(200px,240px)_minmax(0,1fr)]";
pub const MFA_QR_CARD: &str =
    "grid content-start justify-items-start gap-3 min-[901px]:justify-items-center";
pub const MFA_QR: &str = "rounded-[14px] border border-border-subtle bg-white p-4 leading-none shadow-soft [&_svg]:block [&_svg]:h-auto [&_svg]:w-[168px]";
pub const MFA_QR_CAPTION: &str =
    "m-0 text-left text-xs leading-snug text-tertiary min-[901px]:text-center";
pub const MFA_ENROLL_SIDE: &str = "grid min-w-0 gap-[22px]";
pub const MFA_SECRET_ROW: &str = "mt-2.5 grid grid-cols-[minmax(0,1fr)_auto] items-stretch gap-2";
pub const MFA_SECRET: &str = "break-words rounded-[10px] border border-border-subtle bg-surface-subtle p-3 font-mono text-[13px] tracking-wider leading-snug";
pub const MFA_CODE_INPUT: &str = "max-w-[220px] font-mono text-lg tracking-[0.18em]";
pub const MFA_VERIFY_TITLE: &str = "m-0 mt-1.5 text-[15px] font-semibold";
pub const MFA_RECOVERY_GRID: &str = "my-[18px] grid list-none grid-cols-1 gap-x-4 gap-y-2 rounded-xl border border-border-subtle bg-surface-subtle p-4 min-[901px]:grid-cols-2";
pub const MFA_RECOVERY_CODE: &str = "font-mono text-[13px] tracking-wide";
pub const MFA_ACK: &str = "my-4 mb-2 flex items-start gap-2.5";
pub const MFA_ACK_LABEL: &str = "text-[13px] leading-normal text-secondary";
pub const MFA_PRIMARY_MT: &str = "mt-3.5";
pub const MFA_LINK_DISABLED: &str = "pointer-events-none cursor-not-allowed opacity-45";

// ── Passkeys ───────────────────────────────────────────────────────────────

pub const PASSKEY_FLOW: &str = "grid min-w-0 w-full gap-5";
pub const PASSKEY_OVERVIEW: &str = "grid min-w-0 gap-4";
pub const PASSKEY_FOCUS_WRAP: &str = "grid min-w-0";
pub const PASSKEY_FOCUS_PANEL: &str = "grid w-full min-w-0 max-w-[560px] gap-[18px] rounded-[14px] border border-border-subtle bg-surface px-7 pb-8 pt-7 shadow-soft";
pub const PASSKEY_HINT: &str = "m-0 mt-3.5 max-w-[52ch] text-xs leading-normal text-tertiary";
pub const PASSKEY_DEVICE_CARD: &str = "my-[22px] mb-2 grid justify-items-center gap-3 rounded-[14px] border border-border-subtle bg-surface-subtle px-5 py-7";
pub const PASSKEY_DEVICE_CARD_P: &str = "m-0 text-[13px] font-medium text-secondary";
pub const PASSKEY_DEVICE_ICON: &str = "grid h-16 w-12 place-content-center gap-1.5 rounded-[18px] border-2 border-border-strong px-3 py-3.5";
pub const PASSKEY_DEVICE_BAR: &str = "block h-[3px] w-[18px] rounded-full bg-primary";
pub const PASSKEY_DEVICE_BAR_SHORT: &str = "block h-[3px] w-3 rounded-full bg-primary";
pub const PASSKEY_BUTTON_MT: &str = "mt-2";
pub const PASSKEY_ROW_MT: &str = "mt-[18px]";

// ── Providers ──────────────────────────────────────────────────────────────

pub const PROVIDER_CATALOG_GRID: &str = "mt-0 grid grid-cols-1 gap-3 min-[901px]:grid-cols-3";
pub const PROVIDER_CARD: &str = "flex min-h-[72px] items-center gap-3.5 rounded-[14px] border px-4 py-3.5 transition-[border-color,background-color,opacity] duration-150";
pub const PROVIDER_CARD_ON: &str = "border-border-strong bg-surface opacity-100";
pub const PROVIDER_CARD_OFF: &str = "border-border-subtle bg-surface-subtle opacity-[0.72]";
pub const PROVIDER_LOGO: &str = "inline-flex h-11 w-11 flex-none items-center justify-center rounded-xl border border-border-subtle bg-canvas text-primary [&_svg]:block";
pub const PROVIDER_LOGO_OFF: &str = "inline-flex h-11 w-11 flex-none items-center justify-center rounded-xl border border-border-subtle bg-canvas text-tertiary opacity-75 grayscale [&_svg]:block";
pub const PROVIDER_CARD_BODY: &str = "grid min-w-0 gap-0.5";
pub const PROVIDER_NAME: &str = "text-sm font-semibold tracking-tight text-primary";
pub const PROVIDER_STATUS: &str = "text-xs font-medium text-tertiary";
pub const PROVIDER_STATUS_ON: &str = "text-xs font-medium text-success";
pub const PROVIDERS_NOTE: &str = "m-0 pb-0.5 text-left text-sm leading-normal text-secondary";

// ── Vault ──────────────────────────────────────────────────────────────────

pub const VAULT_PAGE: &str = "grid gap-4";
pub const VAULT_LEDE: &str = "mb-3 max-w-[62ch] text-sm leading-normal text-secondary";
pub const VAULT_ACTIONS: &str = "flex flex-wrap gap-2";
pub const VAULT_PANEL_HEAD: &str = "mb-3 flex items-center justify-between gap-3";
pub const VAULT_PANEL_HEAD_TITLE: &str = "m-0 text-[1.05rem] font-semibold";
pub const VAULT_PANEL_HEAD_META: &str = "flex flex-wrap items-center gap-2.5";
pub const VAULT_ADD_INLINE: &str = "min-h-8 px-2.5 py-1 text-xs";
pub const VAULT_TABLE_WRAP: &str = "w-full overflow-x-auto";
pub const VAULT_TABLE: &str =
    "w-full table-fixed border-collapse text-[13px] max-[720px]:min-w-[640px]";
pub const VAULT_TH: &str = "border-b border-border-subtle px-2.5 py-3 text-left align-middle text-[11px] font-semibold uppercase tracking-wide text-secondary";
pub const VAULT_TD: &str = "border-b border-border-subtle px-2.5 py-3 text-left align-middle";
pub const VAULT_TD_ELLIPSIS: &str = "border-b border-border-subtle px-2.5 py-3 text-left align-middle overflow-hidden text-ellipsis whitespace-nowrap";
/// Full actions-column header (do not compose with `VAULT_TH` — avoids text-left/right conflict).
pub const VAULT_TH_ACTIONS: &str = "border-b border-border-subtle px-2.5 py-3 text-right align-middle w-[10%] text-[11px] font-semibold uppercase tracking-wide text-secondary";
/// Full actions-column body cell (standalone; do not compose with `VAULT_TD`).
pub const VAULT_TD_ACTIONS: &str =
    "border-b border-border-subtle px-2.5 py-3 text-right align-middle w-[10%]";
pub const VAULT_EMPTY: &str = "whitespace-normal px-2 py-5";
pub const VAULT_SCOPE: &str = "inline-block rounded-full bg-[color-mix(in_srgb,var(--bg-surface-subtle)_80%,transparent)] px-2 py-0.5 text-[11px]";
pub const VAULT_TD_VALUE: &str =
    "border-b border-border-subtle px-2.5 py-3 text-left align-middle font-mono overflow-hidden";
pub const VAULT_VALUE_INNER: &str = "flex w-full min-w-0 items-center gap-2";
pub const VAULT_MASKED: &str = "flex-none tracking-widest text-secondary";
pub const VAULT_REVEALED: &str = "inline-block min-w-0 max-w-full flex-auto overflow-hidden text-ellipsis whitespace-nowrap break-all rounded-md bg-[color-mix(in_srgb,var(--warning)_12%,var(--bg-surface))] px-2 py-1 text-xs";
pub const VAULT_EYE: &str = "flex-none cursor-pointer rounded-lg border border-border-subtle bg-transparent px-2 py-1 text-sm leading-none hover:bg-surface-subtle";
pub const VAULT_TRASH: &str = "inline-flex cursor-pointer items-center justify-center rounded-lg border border-transparent bg-transparent p-1.5 leading-none text-secondary hover:border-[color-mix(in_srgb,var(--danger)_28%,var(--border-subtle))] hover:bg-[color-mix(in_srgb,var(--danger)_10%,var(--bg-surface))] hover:text-danger";
pub const VAULT_TRASH_ICON: &str = "block h-4 w-4";
pub const VAULT_FORM: &str = "mt-0 grid grid-cols-1 gap-3 min-[721px]:grid-cols-2";
pub const VAULT_FIELD_WIDE: &str = "col-span-full";
pub const VAULT_VALUE_INPUT_ROW: &str = "flex gap-2 [&_input]:min-w-0 [&_input]:flex-1";
pub const VAULT_MODAL_BACKDROP: &str =
    "fixed inset-0 z-[80] grid place-items-center bg-overlay-scrim p-4 py-6 overscroll-contain";
pub const VAULT_MODAL: &str = "grid max-h-[min(84dvh,720px)] w-[min(520px,calc(100vw-32px))] max-w-[520px] grid-rows-[auto_minmax(0,1fr)] gap-[18px] overflow-hidden rounded-[18px] border border-border-subtle bg-surface p-5 shadow-[0_24px_64px_rgba(0,0,0,0.22)]";
pub const VAULT_MODAL_CONFIRM: &str = "grid max-h-[min(84dvh,720px)] w-[min(440px,calc(100vw-32px))] max-w-[440px] grid-rows-[auto_minmax(0,1fr)] gap-[18px] overflow-hidden rounded-[18px] border border-border-subtle bg-surface p-5 shadow-[0_24px_64px_rgba(0,0,0,0.22)]";
pub const VAULT_MODAL_HEAD: &str = "flex items-start justify-between gap-4";
pub const VAULT_MODAL_HEAD_TITLE: &str = "m-0 mb-1 text-lg font-semibold tracking-tight";
pub const VAULT_MODAL_HEAD_P: &str = "m-0 text-[13px] text-secondary";
pub const VAULT_MODAL_CLOSE: &str = "flex-none cursor-pointer rounded-full border border-border-subtle bg-surface-subtle px-3 py-2 text-xs font-semibold text-secondary hover:text-primary";
pub const VAULT_MODAL_BODY: &str = "grid min-h-0 gap-4 overflow-auto overscroll-contain";
pub const VAULT_MODAL_ACTIONS: &str = "flex flex-wrap justify-end gap-2";
pub const VAULT_DANGER_BUTTON: &str =
    "!border-danger !bg-danger !text-white hover:enabled:brightness-95";
pub const VAULT_COL_KEY: &str = "w-[22%] max-[720px]:w-auto";
pub const VAULT_COL_LABEL: &str = "w-[22%] max-[720px]:w-auto";
pub const VAULT_COL_SCOPE: &str = "w-[12%] max-[720px]:w-auto";
pub const VAULT_COL_VALUE: &str = "w-[34%] max-[720px]:w-auto";
pub const VAULT_COL_ACTIONS: &str = "w-[10%] max-[720px]:w-auto";

// ── Profile ────────────────────────────────────────────────────────────────

pub const PROFILE_EDITOR: &str = "mx-auto w-full max-w-none min-w-0 overflow-visible rounded-[14px] border border-border-subtle bg-surface p-0 gap-0 grid";
pub const PROFILE_EDITOR_BODY: &str = "grid gap-0";
pub const PROFILE_LOADING: &str = "flex flex-col items-center justify-center gap-4 px-7 py-9";
pub const PROFILE_SKELETON_AVATAR: &str = "h-24 w-24 rounded-full bg-[#e8e8ed] dark:bg-zinc-700";
pub const PROFILE_SKELETON_LINES: &str = "grid w-full justify-items-center gap-2.5";
pub const PROFILE_SKELETON_LINE: &str = "block h-3 rounded-md bg-[#e8e8ed] dark:bg-zinc-700";
pub const PROFILE_IDENTITY_STRIP: &str = "flex flex-col items-center justify-center gap-3.5 border-b border-border-subtle px-7 pb-7 pt-8 text-center max-[720px]:px-5 max-[720px]:pb-5 max-[720px]:pt-6";
pub const PROFILE_AVATAR_WRAP: &str = "group relative mx-auto h-[104px] w-[104px] flex-none";
pub const PROFILE_AVATAR_CONTROL: &str =
    "group/avatar relative m-0 block h-full w-full cursor-pointer";
pub const PROFILE_FILE_INPUT: &str = "absolute m-[-1px] h-px w-px overflow-hidden whitespace-nowrap border-0 p-0 [clip:rect(0,0,0,0)]";
pub const PROFILE_AVATAR_DISK: &str = "relative block h-full w-full overflow-hidden rounded-full border border-zinc-300 bg-[#e8e8ed] shadow-[0_1px_2px_rgba(0,0,0,0.04),0_8px_24px_rgba(0,0,0,0.05)] transition-[transform,box-shadow] duration-200 ease-[cubic-bezier(0.16,1,0.3,1)] will-change-transform group-hover/avatar:scale-[1.02] group-hover/avatar:shadow-[0_2px_8px_rgba(0,0,0,0.1),0_12px_28px_rgba(0,0,0,0.08)] group-focus-within/avatar:scale-[1.02] group-active/avatar:scale-[0.98] dark:border-zinc-600 dark:bg-zinc-700";
pub const PROFILE_AVATAR_IMG: &str = "block h-full w-full object-cover";
pub const PROFILE_AVATAR_FALLBACK: &str = "flex h-full w-full items-center justify-center bg-[#e8e8ed] text-[28px] font-[650] tracking-wide text-zinc-600 dark:bg-zinc-700 dark:text-zinc-300";
pub const PROFILE_AVATAR_VEIL: &str = "pointer-events-none absolute inset-0 flex items-center justify-center bg-[rgba(15,15,15,0.48)] text-white opacity-0 transition-opacity duration-[180ms] ease-[cubic-bezier(0.16,1,0.3,1)] group-hover/avatar:opacity-100 group-focus-within/avatar:opacity-100";
pub const PROFILE_AVATAR_CAMERA: &str = "block drop-shadow-[0_1px_2px_rgba(0,0,0,0.25)]";
pub const PROFILE_AVATAR_CLEAR: &str = "absolute -right-0.5 -top-0.5 z-[2] inline-flex h-7 w-7 cursor-pointer items-center justify-center rounded-full border border-border-strong bg-elevated p-0 text-secondary shadow-[0_2px_8px_rgba(0,0,0,0.12)] opacity-0 pointer-events-none scale-[0.92] transition-[opacity,transform,color,background-color,border-color] duration-150 group-hover:opacity-100 group-hover:pointer-events-auto group-hover:scale-100 group-focus-within:opacity-100 group-focus-within:pointer-events-auto group-focus-within:scale-100 focus-visible:opacity-100 focus-visible:pointer-events-auto focus-visible:scale-100 hover:border-danger hover:bg-surface hover:text-danger active:scale-95 max-[720px]:opacity-100 max-[720px]:pointer-events-auto max-[720px]:scale-100";
pub const PROFILE_IDENTITY_COPY: &str =
    "flex w-full min-w-0 flex-col items-center gap-1 text-center";
pub const PROFILE_DISPLAY_PREVIEW: &str = "m-0 max-w-full break-words text-[clamp(18px,2.2vw,22px)] font-[650] leading-snug tracking-tight text-zinc-700 dark:text-zinc-200";
pub const PROFILE_HANDLE_PREVIEW: &str = "m-0 text-sm font-medium text-zinc-500 dark:text-zinc-400";
pub const PROFILE_EMAIL_LINE: &str = "m-0 break-words text-[13px] text-zinc-500 dark:text-zinc-400";
pub const PROFILE_SECTIONS: &str = "grid gap-0";
pub const PROFILE_SECTION: &str =
    "grid gap-4 border-b border-border-subtle px-7 py-6 last:border-b-0 max-[720px]:px-5";
pub const PROFILE_SECTION_HEAD_H3: &str = "m-0 mb-1 text-[15px] font-[650] tracking-tight";
pub const PROFILE_SECTION_HEAD_P: &str = "m-0 text-[13px] leading-snug text-secondary";
pub const PROFILE_FORM_GRID: &str = "grid grid-cols-1 gap-3.5 min-[721px]:grid-cols-2";
pub const PROFILE_FIELD_SPAN: &str = "col-span-full";
pub const PROFILE_USERNAME_FIELD: &str = "grid grid-cols-[auto_minmax(0,1fr)] items-center overflow-hidden rounded-[10px] border border-border-strong bg-surface focus-within:border-focus focus-within:shadow-[0_0_0_3px_color-mix(in_srgb,var(--focus-ring)_18%,transparent)]";
pub const PROFILE_USERNAME_AT: &str = "select-none pl-3 font-semibold text-tertiary";
pub const PROFILE_USERNAME_INPUT: &str = "!rounded-none !border-0 !bg-transparent !shadow-none";
pub const PROFILE_SWITCH: &str =
    "m-0 grid cursor-pointer grid-cols-[auto_minmax(0,1fr)] items-start gap-3.5";
pub const PROFILE_SWITCH_INPUT: &str = "peer absolute opacity-0 pointer-events-none";
pub const PROFILE_SWITCH_TRACK: &str = "relative mt-px block h-7 w-12 flex-none rounded-full border border-border-strong bg-surface-active transition-[background-color,border-color] duration-[180ms] peer-checked:border-inverse peer-checked:bg-inverse peer-focus-visible:outline peer-focus-visible:outline-2 peer-focus-visible:outline-offset-3 peer-focus-visible:outline-focus peer-checked:[&>span]:translate-x-5";
pub const PROFILE_SWITCH_THUMB: &str = "absolute left-0.5 top-0.5 block h-[22px] w-[22px] rounded-full bg-elevated shadow-[0_1px_3px_rgba(0,0,0,0.18)] transition-transform duration-[180ms] ease-[cubic-bezier(0.16,1,0.3,1)]";
pub const PROFILE_SWITCH_COPY: &str = "grid gap-0.5";
pub const PROFILE_SWITCH_COPY_STRONG: &str = "text-sm font-semibold";
pub const PROFILE_SWITCH_COPY_SMALL: &str = "text-[13px] leading-snug text-secondary";
pub const PROFILE_PUBLIC_LINK: &str = "mt-1 mb-0 flex flex-wrap items-baseline gap-2";
pub const PROFILE_PUBLIC_LINK_LABEL: &str =
    "text-xs font-semibold uppercase tracking-wide text-tertiary";
pub const PROFILE_PUBLIC_LINK_URL: &str =
    "text-sm font-semibold text-primary no-underline hover:underline";
pub const PROFILE_FOOTER: &str = "flex flex-wrap items-center gap-x-4 gap-y-3 rounded-b-[14px] border-t border-border-subtle bg-transparent px-7 pb-7 pt-5 max-[720px]:px-5";
pub const PROFILE_SAVE_OK: &str = "m-0 px-2.5 py-1.5";
pub const PUBLIC_PROFILE_PANEL: &str = "mx-auto w-full max-w-[420px]";
pub const PUBLIC_PROFILE_HERO: &str =
    "grid justify-items-center gap-[18px] px-3 pb-2 pt-5 text-center";
pub const PUBLIC_PROFILE_AVATAR: &str = "h-[120px] w-[120px] overflow-hidden rounded-full border border-border-subtle shadow-[0_8px_24px_rgba(0,0,0,0.08)]";
pub const PUBLIC_PROFILE_META_TITLE: &str =
    "my-1.5 text-[clamp(24px,3vw,32px)] font-[650] tracking-tight";
pub const PUBLIC_PROFILE_EMPTY: &str = "grid justify-items-center gap-3 px-3 py-7 text-center";
pub const PUBLIC_PROFILE_EMPTY_AVATAR: &str = "flex h-[72px] w-[72px] items-center justify-center rounded-full bg-[#e8e8ed] text-[22px] font-[650] text-zinc-600 dark:bg-zinc-700 dark:text-zinc-300";

/// Workspace page header strip.
pub const WORKSPACE_PAGE_HEADER: &str = "mb-7 border-b border-border-subtle px-0 pb-[22px] pt-2";
pub const WORKSPACE_PAGE_TITLE: &str =
    "m-0 max-w-[20ch] text-[clamp(28px,3.8vw,36px)] font-semibold leading-[1.08] tracking-tight";
pub const WORKSPACE_PAGE_SUBTITLE: &str = "text-sm text-secondary";
/// Public / workspace page body grid (legacy `.page-grid`: 2-col, collapse ≤720).
pub const PAGE_GRID: &str =
    "grid min-w-0 grid-cols-2 items-start gap-4 max-[720px]:grid-cols-1";

/// Error interrupt page.
pub const ERROR_PAGE: &str = "grid min-h-dvh place-items-center bg-canvas p-6";
pub const ERROR_CARD: &str =
    "w-full max-w-[480px] rounded-[14px] border border-border-subtle bg-surface p-8 shadow-soft";
pub const ERROR_TITLE: &str = "m-0 text-[clamp(30px,6vw,42px)] font-semibold tracking-tight";
pub const ERROR_COPY: &str = "mt-3 mb-0 max-w-[34ch] text-[15px] leading-relaxed text-secondary";
pub const ERROR_ACTIONS: &str = "mt-6 flex flex-wrap gap-2";

// ── Filter combobox ────────────────────────────────────────────────────────

/// Toolbar-friendly combobox shell (audit filters flex row + mobile full width).
pub const COMBOBOX: &str = "relative min-w-[11rem] flex-[0_1_14rem] max-[720px]:min-w-0 max-[720px]:flex-[1_1_100%]";
pub const COMBOBOX_LABEL: &str = "m-0 grid gap-[0.3rem] text-[0.85rem] font-semibold [&_span]:text-[0.75rem] [&_span]:font-semibold [&_span]:uppercase [&_span]:tracking-[0.02em] [&_span]:text-secondary";
pub const COMBOBOX_CONTROL: &str = "relative";
pub const COMBOBOX_INPUT: &str = "w-full min-h-[42px] appearance-none rounded-[10px] border border-border-strong bg-surface py-2.5 pl-3 pr-10 text-[13px] text-primary outline-none placeholder:text-tertiary focus:border-focus focus:shadow-[0_0_0_3px_color-mix(in_srgb,var(--focus-ring)_18%,transparent)] disabled:cursor-not-allowed disabled:opacity-55";
pub const COMBOBOX_CHEVRON: &str = "pointer-events-none absolute right-0 top-0 h-full w-9 bg-[length:12px_8px] bg-center bg-no-repeat [background-image:url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='8' viewBox='0 0 12 8' fill='none'%3E%3Cpath d='M1.5 1.75L6 6.25L10.5 1.75' stroke='%2364748b' stroke-width='1.5' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E\")]";
pub const COMBOBOX_LIST: &str = "absolute left-0 right-0 top-full z-30 m-0 mt-1 max-h-64 list-none overflow-auto rounded-[10px] border border-border-subtle bg-elevated p-[0.3rem] shadow-soft";
pub const COMBOBOX_EMPTY: &str = "px-[0.65rem] py-[0.55rem] text-[0.85rem] text-secondary";
pub const COMBOBOX_OPTION: &str = "cursor-pointer rounded-lg px-[0.65rem] py-2 text-[0.9rem] font-medium text-primary hover:bg-surface-hover";
pub const COMBOBOX_OPTION_ACTIVE: &str = "bg-surface-hover";
pub const COMBOBOX_OPTION_SELECTED: &str = "font-[650]";

// ── Dashboard board ────────────────────────────────────────────────────────

/// Board page root (legacy `.board-page`).
pub const BOARD_PAGE: &str = "grid w-full min-w-0 gap-[22px]";

/// Header strip: copy + actions.
pub const BOARD_TOP: &str =
    "grid items-end gap-[18px] grid-cols-[minmax(0,1fr)_auto] max-[720px]:grid-cols-1";
pub const BOARD_TOP_COPY: &str = "min-w-0";
pub const BOARD_KICKER: &str =
    "m-0 mb-2 text-[11px] font-[650] uppercase tracking-[0.08em] text-tertiary";
pub const BOARD_TITLE: &str =
    "m-0 text-[clamp(28px,3.4vw,40px)] font-[650] leading-[1.08] tracking-[-0.045em]";
pub const BOARD_SUB: &str = "mt-2.5 mb-0 max-w-[56ch] text-[15px] leading-[1.55] text-secondary";
pub const BOARD_TOP_ACTIONS: &str =
    "flex flex-wrap items-center justify-end gap-2.5 max-[720px]:justify-stretch [&_button]:max-[720px]:flex-auto";

/// Inverse active state for secondary edit toggle (legacy `.secondary-button.is-active`).
pub const BTN_SECONDARY_ACTIVE: &str = "inline-flex items-center justify-center min-h-[42px] rounded-[10px] px-3.5 py-2.5 text-[13px] font-semibold no-underline cursor-pointer transition-[background-color,border-color,color,transform,opacity] duration-[180ms] ease-in-out bg-inverse text-on-inverse border border-inverse hover:bg-[color-mix(in_srgb,var(--bg-inverse)_82%,var(--text-primary))] active:translate-y-px disabled:cursor-wait disabled:opacity-55";

pub const BOARD_EDIT_HINT: &str =
    "m-0 rounded-xl border border-border-subtle bg-surface-subtle px-3.5 py-2.5 text-[13px] leading-[1.45] text-secondary";

/// Dashboard board modal chrome. Settings use `VAULT_MODAL_*`; org create uses `ORG_CREATE_*`.
/// Scroll lock still shares the `board-modal-open` class name with org create.
pub const BOARD_MODAL_BACKDROP: &str =
    "fixed inset-0 z-[80] grid place-items-center bg-overlay-scrim px-4 py-6 overscroll-contain";
pub const BOARD_MODAL: &str = "grid max-h-[min(84dvh,720px)] w-[min(720px,100%)] max-w-[720px] grid-rows-[auto_minmax(0,1fr)] gap-[18px] overflow-hidden rounded-[18px] border border-border-subtle bg-surface p-5 shadow-[0_24px_64px_rgba(0,0,0,0.22)]";
pub const BOARD_MODAL_RESOURCES: &str = "grid max-h-[min(90dvh,860px)] w-[min(1040px,100%)] max-w-[1040px] grid-rows-[auto_minmax(0,1fr)] gap-[18px] overflow-hidden rounded-[18px] border border-border-subtle bg-surface p-5 shadow-[0_24px_64px_rgba(0,0,0,0.22)]";
/// Non-scrolling chrome stack (head + tabs + banners) — pair with body as the sole `1fr` row.
pub const BOARD_MODAL_CHROME: &str = "grid min-h-0 gap-[18px]";
pub const BOARD_MODAL_HEAD: &str = "flex items-start justify-between gap-4";
pub const BOARD_MODAL_HEAD_TITLE: &str = "m-0 mb-1 text-lg font-[650] tracking-tight";
pub const BOARD_MODAL_HEAD_P: &str = "m-0 text-[13px] text-secondary";
pub const BOARD_MODAL_CLOSE: &str = "flex-none cursor-pointer rounded-full border border-border-subtle bg-surface-subtle px-3 py-2 text-xs font-[650] text-secondary hover:text-primary";

pub const BOARD_PICKER_GRID: &str =
    "grid min-h-0 grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-2.5 overflow-auto overscroll-contain";
pub const BOARD_PICKER_CARD: &str =
    "grid min-h-[132px] grid-rows-[1fr_auto] items-end gap-3 rounded-[14px] border border-border-subtle bg-surface-subtle p-3.5";
/// Extra for already-placed single-instance widgets (`with_extra(BOARD_PICKER_CARD, …)`).
pub const BOARD_PICKER_CARD_ADDED: &str = "opacity-[0.72]";
pub const BOARD_PICKER_CARD_TITLE: &str =
    "mb-1 block text-sm font-[650] tracking-tight";
pub const BOARD_PICKER_CARD_P: &str = "m-0 text-xs leading-[1.45] text-secondary";
pub const BOARD_PICKER_BADGE: &str =
    "mt-2 inline-block text-[11px] font-[650] uppercase tracking-[0.04em] text-tertiary";

/// 12-col board grid (inline `style` sets per-tile span; mobile forces full width).
pub const BOARD_GRID: &str =
    "grid min-w-0 grid-cols-12 gap-3.5 max-[960px]:grid-cols-2 max-[720px]:grid-cols-1";
pub const BOARD_NODE_SLOT: &str = "contents";

/// Tile shell base — column span comes from reactive inline `grid-column: span N`.
pub const BOARD_TILE: &str = "grid min-h-[168px] min-w-0 gap-3 rounded-[18px] border border-border-subtle bg-surface px-4 pb-[18px] pt-4 transition-[border-color,box-shadow,transform] duration-[160ms] ease-in-out max-[720px]:![grid-column:1/-1]";
/// State extras for `with_extra(BOARD_TILE, Some(…))`.
pub const BOARD_TILE_EDITING: &str = "cursor-grab shadow-[0_0_0_1px_color-mix(in_srgb,var(--focus-ring)_35%,transparent)] active:cursor-grabbing";
pub const BOARD_TILE_DROP_TARGET: &str = "cursor-grab !border-focus shadow-[0_0_0_2px_color-mix(in_srgb,var(--focus-ring)_28%,transparent)]";

/// Container shell base (same span / mobile rules as tiles).
pub const BOARD_CONTAINER: &str = "grid min-h-[168px] min-w-0 content-start gap-3 rounded-[18px] border border-border-subtle bg-[color-mix(in_srgb,var(--bg-surface-subtle)_55%,var(--bg-surface))] px-4 pb-[18px] pt-4 transition-[border-color,box-shadow,transform] duration-[160ms] ease-in-out max-[720px]:![grid-column:1/-1]";
pub const BOARD_CONTAINER_EDITING: &str = "cursor-grab shadow-[0_0_0_1px_color-mix(in_srgb,var(--focus-ring)_35%,transparent)] active:cursor-grabbing";
pub const BOARD_CONTAINER_DROP_TARGET: &str = "cursor-grab !border-focus shadow-[0_0_0_2px_color-mix(in_srgb,var(--focus-ring)_28%,transparent)]";
pub const BOARD_CONTAINER_HEAD: &str = "flex items-center justify-between gap-2.5";
pub const BOARD_CONTAINER_BODY: &str = "grid min-w-0 grid-cols-12 gap-3";
pub const BOARD_CONTAINER_BODY_STACK: &str = "grid min-w-0 grid-cols-1 gap-3 [&>*]:![grid-column:1/-1]";

/// Compose tile shell + optional editing / drop-target extras (drop wins).
pub fn board_tile_class(editing: bool, drop_target: bool) -> String {
    if drop_target {
        with_extra(BOARD_TILE, Some(BOARD_TILE_DROP_TARGET))
    } else if editing {
        with_extra(BOARD_TILE, Some(BOARD_TILE_EDITING))
    } else {
        BOARD_TILE.to_owned()
    }
}

/// Compose container shell + optional editing / drop-target extras (drop wins).
pub fn board_container_class(editing: bool, drop_target: bool) -> String {
    if drop_target {
        with_extra(BOARD_CONTAINER, Some(BOARD_CONTAINER_DROP_TARGET))
    } else if editing {
        with_extra(BOARD_CONTAINER, Some(BOARD_CONTAINER_EDITING))
    } else {
        BOARD_CONTAINER.to_owned()
    }
}

pub const BOARD_TILE_HEAD: &str = "flex items-start justify-between gap-2.5";
pub const BOARD_TILE_HEAD_MAIN: &str = "flex min-w-0 items-center gap-2";
pub const BOARD_DRAG_HANDLE: &str =
    "cursor-grab select-none text-sm leading-none tracking-[-0.08em] text-tertiary";
pub const BOARD_TILE_KICKER: &str =
    "m-0 text-[11px] font-[650] uppercase tracking-[0.07em] text-tertiary";
pub const BOARD_TILE_CONTROLS: &str = "flex flex-wrap items-center justify-end gap-2";
pub const BOARD_SPAN_GROUP: &str =
    "inline-flex gap-0.5 rounded-full border border-border-subtle bg-surface-subtle p-0.5";
pub const BOARD_SPAN_CHIP: &str =
    "min-w-8 cursor-pointer appearance-none rounded-full border-0 bg-transparent px-2 py-1 text-[11px] font-bold text-tertiary hover:text-primary";
pub const BOARD_SPAN_CHIP_ACTIVE: &str =
    "min-w-8 cursor-pointer appearance-none rounded-full border-0 bg-surface px-2 py-1 text-[11px] font-bold text-primary shadow-[0_1px_2px_rgba(0,0,0,0.08)]";
pub const BOARD_TILE_REMOVE: &str = "inline-flex h-7 w-7 cursor-pointer appearance-none items-center justify-center rounded-full border border-border-subtle bg-surface-subtle p-0 text-secondary hover:border-danger hover:text-danger";
pub const BOARD_TILE_BODY: &str = "min-w-0";
pub const BOARD_TILE_BODY_DIMMED: &str = "min-w-0 opacity-[0.92] pointer-events-none";

pub const BOARD_METRIC: &str = "grid gap-1.5 pt-1";
pub const BOARD_METRIC_VALUE: &str =
    "inline-flex items-center gap-2 text-[22px] font-[650] tracking-tight";
pub const BOARD_METRIC_NUMBER: &str =
    "inline-flex items-center gap-2 text-[34px] font-[650] tabular-nums tracking-[-0.04em]";
pub const BOARD_METRIC_META: &str = "text-[13px] text-secondary";
pub const BOARD_PULSE: &str = "inline-block h-2 w-2 rounded-full bg-success shadow-[0_0_0_0_color-mix(in_srgb,var(--success)_40%,transparent)] animate-[board-pulse_2.2s_ease-in-out_infinite]";
pub const BOARD_SCORE_BAR: &str =
    "mt-2 h-1.5 overflow-hidden rounded-full bg-surface-subtle";
pub const BOARD_SCORE_BAR_FILL: &str = "block h-full rounded-full bg-inverse";

pub const BOARD_LIST: &str = "m-0 grid list-none gap-0 p-0";
pub const BOARD_LIST_ROW: &str = "grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-2.5 border-b border-border-subtle py-2.5 last:border-b-0";
pub const BOARD_LIST_GROW: &str = "grid min-w-0 gap-0.5";
pub const BOARD_LIST_STRONG: &str = "text-[13px] font-[650] tracking-tight";
pub const BOARD_LIST_META: &str = "text-xs text-tertiary";

pub const BOARD_FEED: &str = "m-0 grid list-none gap-0 p-0";
pub const BOARD_FEED_ITEM: &str =
    "grid grid-cols-[auto_minmax(0,1fr)] items-center gap-2.5 border-b border-border-subtle py-2.5 last:border-b-0";
pub const BOARD_FEED_DOT: &str = "mt-1.5 h-2 w-2 rounded-full bg-tertiary";
pub const BOARD_FEED_DOT_OK: &str = "mt-1.5 h-2 w-2 rounded-full bg-success";
pub const BOARD_FEED_DOT_ERR: &str = "mt-1.5 h-2 w-2 rounded-full bg-danger";
pub const BOARD_FEED_COPY: &str = "grid min-w-0 gap-0.5";

pub const BOARD_PILL: &str =
    "whitespace-nowrap rounded-full border border-border-subtle bg-surface-subtle px-2 py-[3px] text-[11px] font-[650] text-secondary";
pub const BOARD_PILL_LIVE: &str = "whitespace-nowrap rounded-full border border-[color-mix(in_srgb,var(--success)_28%,var(--border-subtle))] bg-[color-mix(in_srgb,var(--success)_14%,var(--bg-surface))] px-2 py-[3px] text-[11px] font-[650] text-success";

pub const BOARD_NOTIF_LIST: &str = "m-0 grid list-none gap-0 p-0";
pub const BOARD_NOTIF: &str =
    "grid grid-cols-[minmax(0,1fr)_auto] items-start gap-2.5 border-b border-border-subtle py-3 last:border-b-0";
pub const BOARD_NOTIF_WARN: &str =
    "grid grid-cols-[minmax(0,1fr)_auto] items-start gap-2.5 border-b border-border-subtle border-l-2 border-l-warning py-3 pl-2.5 last:border-b-0";
pub const BOARD_NOTIF_INFO: &str =
    "grid grid-cols-[minmax(0,1fr)_auto] items-start gap-2.5 border-b border-border-subtle border-l-2 border-l-border-strong py-3 pl-2.5 last:border-b-0";
pub const BOARD_NOTIF_COPY: &str = "min-w-0";
pub const BOARD_NOTIF_TITLE: &str = "mb-[3px] block text-[13px] font-[650]";
pub const BOARD_NOTIF_BODY: &str = "m-0 text-[13px] leading-[1.45] text-secondary";
pub const BOARD_NOTIF_TIME: &str = "mt-1.5 block text-[11px] text-tertiary";
pub const BOARD_NOTIF_DISMISS: &str =
    "cursor-pointer appearance-none border-0 bg-transparent p-0.5 text-xs font-semibold text-tertiary hover:text-primary";

pub const BOARD_SECURITY: &str = "grid gap-3";
pub const BOARD_SECURITY_SCORE: &str = "flex items-baseline gap-2";
pub const BOARD_SECURITY_SCORE_VALUE: &str =
    "text-[32px] font-[650] tabular-nums tracking-[-0.04em]";
pub const BOARD_SECURITY_SCORE_LABEL: &str = "text-[13px] text-tertiary";

pub const BOARD_CHECKLIST: &str = "m-0 grid list-none gap-2 p-0";
pub const BOARD_CHECKLIST_ITEM: &str = "relative pl-[18px] text-[13px] text-secondary before:absolute before:left-0 before:top-[5px] before:h-2 before:w-2 before:rounded-full before:bg-border-strong before:content-['']";
pub const BOARD_CHECKLIST_ITEM_DONE: &str = "relative pl-[18px] text-[13px] text-primary before:absolute before:left-0 before:top-[5px] before:h-2 before:w-2 before:rounded-full before:bg-success before:content-['']";
pub const BOARD_CHECKLIST_LG: &str = "m-0 grid list-none gap-2 p-0 [&_a]:text-inherit [&_a]:no-underline hover:[&_a]:underline";

pub const BOARD_INLINE_LINK: &str =
    "mt-2.5 inline-flex text-xs font-[650] text-secondary no-underline hover:text-primary hover:underline";
pub const BOARD_INLINE_LINKS: &str = "flex flex-wrap gap-3";
pub const BOARD_INLINE_LINK_FLUSH: &str =
    "inline-flex text-xs font-[650] text-secondary no-underline hover:text-primary hover:underline";

pub const BOARD_NOTES: &str = "grid gap-2.5";
pub const BOARD_NOTES_INPUT: &str = "w-full min-h-[110px] resize-y rounded-xl border border-border-subtle bg-surface-subtle p-3 font-inherit leading-normal text-primary outline-none focus:border-focus";

pub const BOARD_EMPTY_TILE: &str = "grid justify-items-start gap-2.5 text-secondary";
pub const BOARD_EMPTY: &str = "grid justify-items-start gap-2.5 text-secondary";
pub const BOARD_EMPTY_BOARD: &str = "col-span-full grid justify-items-center gap-2.5 rounded-[18px] border border-dashed border-border-strong bg-surface px-6 py-12 text-center text-secondary";
pub const BOARD_EMPTY_BOARD_TITLE: &str = "m-0 text-xl font-[650] text-primary";

pub const BOARD_SKELETON: &str = "grid gap-4";
pub const BOARD_SKELETON_BAR: &str = "h-[88px] rounded-[14px] bg-surface-subtle";
pub const BOARD_SKELETON_GRID: &str =
    "grid grid-cols-4 gap-3 max-[720px]:grid-cols-2 [&_span]:block [&_span]:h-[140px] [&_span]:rounded-2xl [&_span]:bg-surface-subtle";
pub const BOARD_SKELETON_SPAN2: &str = "col-span-2 !h-[180px]";

pub const BOARD_BIND_EDITOR: &str =
    "mb-2 grid gap-2 rounded-xl border border-border-subtle bg-surface-subtle p-2.5";
pub const BOARD_BIND_FIELD: &str = "grid gap-1";
pub const BOARD_BIND_FIELD_LABEL: &str =
    "text-[11px] font-[650] uppercase tracking-[0.04em] text-tertiary";
pub const BOARD_BIND_ROW: &str = "grid grid-cols-2 gap-2 max-[720px]:grid-cols-1";
pub const BOARD_BIND_HINT: &str = "m-0 text-[11px] text-tertiary";

pub const BOARD_TABLE_WRAP: &str = "max-h-[280px] overflow-auto";
pub const BOARD_TABLE: &str = "w-full border-collapse text-xs";
pub const BOARD_TABLE_TH: &str =
    "border-b border-border-subtle px-2 py-1.5 text-left align-top text-[10px] font-bold uppercase tracking-[0.05em] text-tertiary";
pub const BOARD_TABLE_TD: &str =
    "border-b border-border-subtle px-2 py-1.5 text-left align-top";

// ── Resources & queries modal ──────────────────────────────────────────────

pub const BOARD_RQ_TABS: &str = "flex flex-wrap gap-1.5";
pub const BOARD_RQ_TAB: &str = "cursor-pointer appearance-none rounded-full border border-border-subtle bg-surface-subtle px-3 py-1.5 text-xs font-[650] text-secondary";
pub const BOARD_RQ_TAB_ACTIVE: &str = "cursor-pointer appearance-none rounded-full border border-inverse bg-inverse px-3 py-1.5 text-xs font-[650] text-on-inverse";
pub const BOARD_RQ_BODY: &str = "grid min-h-0 gap-4 overflow-auto overscroll-contain";
pub const BOARD_RQ_CATALOG: &str =
    "grid grid-cols-2 gap-3 max-[720px]:grid-cols-1";
pub const BOARD_RQ_CARD: &str =
    "grid gap-2 rounded-[14px] border border-border-subtle bg-surface-subtle p-3.5";
pub const BOARD_RQ_CARD_DISABLED: &str =
    "grid gap-2 rounded-[14px] border border-border-subtle bg-surface-subtle p-3.5 opacity-55";
pub const BOARD_RQ_CARD_TITLE: &str = "text-sm font-[650]";
pub const BOARD_RQ_CARD_P: &str = "m-0 text-xs leading-[1.45] text-secondary";
pub const BOARD_RQ_LISTS: &str = "grid grid-cols-2 gap-4 max-[720px]:grid-cols-1";
pub const BOARD_RQ_FORM: &str = "grid gap-3";
pub const BOARD_RQ_FORM_H3: &str = "m-0 mb-2.5 text-sm font-[650]";
pub const BOARD_RQ_ROW: &str = "grid grid-cols-2 gap-2.5 max-[720px]:grid-cols-1";
pub const BOARD_RQ_TEXTAREA: &str = "min-h-[88px] resize-y";
pub const BOARD_RQ_CHECK: &str = "flex items-center gap-2 text-[13px]";
pub const BOARD_RQ_ACTIONS: &str = "flex flex-wrap gap-2";
pub const BOARD_RQ_OUTPUT: &str = "mt-2 grid gap-2";
pub const BOARD_JSON_PREVIEW_LG: &str =
    "max-h-60 overflow-auto whitespace-pre-wrap break-words rounded-[10px] border border-border-subtle bg-surface-subtle p-2.5 text-[11px]";

// ── Workspace settings (domain pages; shell chrome stays residual CSS) ─────

/// Page chrome inside settings outlet.
pub const WS_PAGE: &str = "min-w-0";
pub const WS_PAGE_HEADER: &str = "mb-[18px] grid gap-1.5";
pub const WS_PAGE_TITLE: &str =
    "m-0 text-[28px] font-bold leading-[1.15] tracking-[-0.02em]";
pub const WS_PAGE_SUB: &str = "m-0 text-sm leading-snug text-secondary";
pub const WS_STUB_PANEL: &str =
    "grid gap-3.5 min-w-0 rounded-[14px] border border-border-subtle bg-surface p-6";
pub const WS_STUB_RESULT: &str = "m-0 mb-3 text-[13px] leading-normal text-secondary";
pub const WS_EMPTY: &str = "grid gap-1.5 text-secondary [&_p]:m-0";
pub const WS_REDIRECT: &str = "mx-auto my-6 max-w-[480px]";

/// Key/value grid (legacy `.kv` + settings density).
pub const WS_KV: &str =
    "mb-3 m-0 grid grid-cols-[minmax(100px,max-content)_minmax(0,1fr)] gap-x-4 gap-y-2.5";
pub const WS_READONLY_TAG: &str = "text-xs font-medium text-tertiary";

/// Step-up AAL2 callout.
pub const WS_STEP_UP: &str = "mb-5 mt-0 rounded-lg border border-[color-mix(in_srgb,#f59e0b_35%,transparent)] bg-[color-mix(in_srgb,#f59e0b_12%,transparent)] px-3 py-2.5 text-[13px] leading-snug text-secondary [&_a]:font-semibold [&_a]:text-inherit [&_a]:underline [&_a]:underline-offset-2";

pub const WS_GENERAL_FORM: &str = "grid max-w-[36rem] gap-3.5";

/// Settings tables with mobile card layout (data-label ::before).
pub const WS_TABLE_WRAP: &str = "m-0 w-full overflow-x-auto max-[720px]:overflow-visible";
pub const WS_TABLE: &str = "w-full min-w-full table-auto border-separate border-spacing-0 text-[13px] leading-snug [&_tbody_tr:last-child_td]:border-b-0 [&_tbody_tr:hover_td]:bg-[color-mix(in_srgb,var(--text-primary)_2.5%,transparent)] max-[720px]:grid max-[720px]:min-w-0 max-[720px]:gap-3 max-[720px]:[&_tbody]:grid max-[720px]:[&_tbody]:min-w-0 max-[720px]:[&_tbody]:w-full max-[720px]:[&_tbody]:gap-3 max-[720px]:[&_tbody_tr:hover_td]:bg-transparent";
pub const WS_TABLE_AUDIT: &str = "w-full min-w-full table-fixed border-separate border-spacing-0 text-[13px] leading-snug [&_tbody_tr:last-child_td]:border-b-0 [&_tbody_tr:hover_td]:bg-[color-mix(in_srgb,var(--text-primary)_2.5%,transparent)] max-[720px]:grid max-[720px]:min-w-0 max-[720px]:gap-3 max-[720px]:[&_tbody]:grid max-[720px]:[&_tbody]:min-w-0 max-[720px]:[&_tbody]:w-full max-[720px]:[&_tbody]:gap-3 max-[720px]:[&_tbody_tr:hover_td]:bg-transparent";
pub const WS_THEAD: &str = "max-[720px]:absolute max-[720px]:m-[-1px] max-[720px]:h-px max-[720px]:w-px max-[720px]:overflow-hidden max-[720px]:whitespace-nowrap max-[720px]:border-0 max-[720px]:p-0 max-[720px]:[clip:rect(0,0,0,0)]";
pub const WS_TR: &str = "max-[720px]:grid max-[720px]:w-full max-[720px]:gap-0 max-[720px]:rounded-xl max-[720px]:border max-[720px]:border-border-subtle max-[720px]:bg-[color-mix(in_srgb,var(--bg-surface)_92%,var(--bg-canvas))] max-[720px]:px-[0.85rem] max-[720px]:pb-[0.55rem] max-[720px]:pt-[0.35rem] max-[720px]:shadow-[0_1px_2px_rgba(15,23,42,0.04)]";
pub const WS_TH: &str = "box-border border-b border-border-subtle px-[0.9rem] pb-[0.7rem] pt-[0.85rem] text-left align-middle text-[11px] font-[650] uppercase tracking-[0.04em] text-secondary whitespace-nowrap";
/// Body cell — desktop table + mobile card row with `data-label` pseudo.
/// Desktop padding is explicit `pt`/`px`/`pb` (no conflicting `py` + `pb`).
pub const WS_TD: &str = "box-border border-b border-border-subtle px-[0.9rem] pt-[0.85rem] pb-4 text-left align-middle max-[720px]:flex max-[720px]:min-w-0 max-[720px]:items-start max-[720px]:justify-between max-[720px]:gap-[0.85rem] max-[720px]:whitespace-normal max-[720px]:border-b max-[720px]:border-[color-mix(in_srgb,var(--border-subtle)_80%,transparent)] max-[720px]:px-0 max-[720px]:pt-[0.65rem] max-[720px]:pb-[0.65rem] max-[720px]:last:border-b-0 max-[720px]:before:content-[attr(data-label)] max-[720px]:before:flex-none max-[720px]:before:max-w-[38%] max-[720px]:before:pt-[0.15rem] max-[720px]:before:text-[0.72rem] max-[720px]:before:font-[650] max-[720px]:before:uppercase max-[720px]:before:tracking-[0.04em] max-[720px]:before:leading-snug max-[720px]:before:text-secondary max-[720px]:[&>*]:min-w-0 max-[720px]:[&>*]:text-right";

pub const WS_MEMBER_EMAIL: &str = "mr-2";
pub const WS_YOU_BADGE: &str = "inline-block rounded-full bg-[color-mix(in_srgb,var(--text-primary)_10%,transparent)] px-2 py-0.5 text-[11px] font-semibold uppercase tracking-[0.02em] text-secondary";

/// Native select with custom chevron (settings role / transfer / invite).
pub const WS_SELECT: &str = "box-border max-w-[14rem] min-h-[42px] min-w-[8.5rem] w-auto cursor-pointer appearance-none rounded-[10px] border border-border-strong bg-surface bg-[length:12px_8px] bg-[position:right_0.85rem_center] bg-no-repeat py-2.5 pl-3 pr-10 text-primary [background-image:url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='8' viewBox='0 0 12 8' fill='none'%3E%3Cpath d='M1.5 1.75L6 6.25L10.5 1.75' stroke='%2364748b' stroke-width='1.5' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E\")] max-[720px]:max-w-[min(14rem,58vw)]";
pub const WS_SELECT_LABEL: &str = "m-0 inline-flex";

pub const WS_MEMBER_ACTIONS: &str = "min-w-[10rem] whitespace-nowrap max-[720px]:flex max-[720px]:min-w-0 max-[720px]:flex-wrap max-[720px]:justify-end max-[720px]:gap-2 max-[720px]:whitespace-normal";
pub const WS_REMOVE_BUTTON: &str = "px-2.5 py-1.5 text-[13px]";
pub const WS_SELF_REMOVE_HINT: &str = "inline-block max-w-[14rem] whitespace-normal text-xs";

pub const WS_MODAL_ACTIONS: &str = "flex flex-wrap justify-end gap-2.5";

/// Danger / destructive filled button (compose with `BTN_PRIMARY` / `BTN_SECONDARY`).
pub const WS_DANGER_BUTTON: &str =
    "!border-[#b91c1c] !bg-[#b91c1c] !text-white hover:enabled:!border-[#991b1b] hover:enabled:!bg-[#991b1b] disabled:!text-white/90";
/// Danger card action: content-sized desktop, full-width touch targets ≤720px.
pub const WS_DANGER_CARD_BTN: &str =
    "max-w-full w-auto self-start max-[720px]:w-full max-[720px]:self-stretch";

pub const WS_DANGER_ZONES: &str = "flex flex-col gap-5";
pub const WS_DANGER_CARD: &str = "flex flex-col items-start gap-3 rounded-[10px] border border-[color-mix(in_srgb,var(--danger)_28%,var(--border-subtle))] bg-[color-mix(in_srgb,var(--danger)_6%,var(--bg-surface))] px-[1.15rem] py-4 [&_h3]:m-0 [&_h3]:w-full [&_h3]:max-w-full [&_h3]:text-[1.05rem] [&_p]:w-full [&_p]:max-w-full";
pub const WS_DANGER_CONFIRM: &str = "flex w-full max-w-[28rem] flex-col items-start gap-3 [&_label]:w-full";

pub const WS_TRANSFER_BAR: &str = "mb-[1.35rem] mt-0 flex flex-wrap items-center justify-between gap-3 rounded-[10px] border border-border-subtle bg-surface px-4 py-[0.85rem] max-[720px]:flex-col max-[720px]:items-stretch [&_p]:m-0 [&_p]:flex-[1_1_16rem] max-[720px]:[&_button]:w-full";
/// Transfer ownership modal body (standalone — do not compose with `VAULT_MODAL_BODY`).
pub const WS_TRANSFER_MODAL_BODY: &str =
    "flex min-h-0 flex-col gap-[0.85rem] overflow-auto overscroll-contain";

pub const WS_INVITE_FORM: &str = "mb-5 mt-[0.15rem] grid max-w-[36rem] gap-[0.85rem]";
pub const WS_INVITE_ACTIONS: &str = "flex items-center gap-3";
pub const WS_INVITATION_ACTIONS: &str = "flex flex-wrap items-center gap-2 max-[720px]:justify-end";

/// Status pill layout/type only — colors live on `WS_STATUS_*` variants (no color clash with `with_extra`).
pub const WS_STATUS_PILL: &str =
    "inline-flex items-center rounded-full px-2 py-[0.1rem] text-xs font-semibold lowercase";
pub const WS_STATUS_NEUTRAL: &str =
    "bg-[color-mix(in_srgb,var(--border-subtle)_70%,transparent)] text-primary";
pub const WS_STATUS_PENDING: &str =
    "bg-[color-mix(in_srgb,#2563eb_16%,transparent)] text-[#1d4ed8]";
pub const WS_STATUS_MUTED: &str =
    "bg-[color-mix(in_srgb,#94a3b8_22%,transparent)] text-[#475569]";
pub const WS_STATUS_ACCEPTED: &str =
    "bg-[color-mix(in_srgb,#0f7b58_16%,transparent)] text-[#0f7b58]";
pub const WS_STATUS_FAIL: &str =
    "bg-[color-mix(in_srgb,#dc2626_14%,transparent)] text-[#b91c1c]";

/// Invitation / audit status pill from raw status string.
pub fn ws_status_pill(status: &str) -> String {
    let colors = match status {
        "pending" => WS_STATUS_PENDING,
        "revoked" | "expired" => WS_STATUS_MUTED,
        "accepted" | "succeeded" | "allowed" => WS_STATUS_ACCEPTED,
        "failed" | "denied" => WS_STATUS_FAIL,
        _ => WS_STATUS_NEUTRAL,
    };
    with_extra(WS_STATUS_PILL, Some(colors))
}

pub const WS_ROLES_TOOLBAR: &str = "mb-5 mt-0 flex w-full flex-wrap justify-start gap-3";
pub const WS_ROLE_NAME: &str = "grid gap-[0.15rem] [&_strong]:font-semibold [&_small]:text-xs";
pub const WS_ROLE_BADGE: &str =
    "inline-flex items-center rounded-full px-[0.55rem] py-[0.12rem] text-[0.72rem] font-semibold uppercase tracking-[0.02em]";
pub const WS_ROLE_BADGE_BUILTIN: &str =
    "bg-[color-mix(in_srgb,#475569_14%,transparent)] text-[#334155]";
pub const WS_ROLE_BADGE_CUSTOM: &str =
    "bg-[color-mix(in_srgb,#7c3aed_14%,transparent)] text-[#6d28d9]";
pub const WS_ROLE_ACTIONS: &str = "flex flex-wrap items-center gap-2 max-[720px]:justify-end";

pub const WS_ROLE_FORM: &str =
    "mb-5 mt-0 grid max-w-[48rem] gap-[0.9rem] rounded-xl border border-border-subtle px-[1.1rem] pt-4 pb-[1.15rem]";
pub const WS_ROLE_FORM_HEAD_H2: &str = "mb-1 mt-0 text-[1.05rem]";
pub const WS_ROLE_FORM_HEAD_P: &str = "m-0";
pub const WS_PERMISSION_FIELDSET: &str = "m-0 min-w-0 border-0 p-0";
pub const WS_PERMISSION_LEGEND: &str = "mb-2 p-0 text-[0.85rem] font-semibold";
pub const WS_PERMISSION_GROUP: &str = "mb-[0.85rem] grid gap-[0.45rem]";
pub const WS_PERMISSION_GROUP_H3: &str =
    "m-0 text-[0.8rem] font-semibold uppercase tracking-[0.03em] text-secondary";
pub const WS_PERMISSION_GRID: &str =
    "grid grid-cols-[repeat(auto-fill,minmax(14rem,1fr))] gap-[0.45rem]";
pub const WS_PERMISSION_OPTION: &str = "flex cursor-pointer items-start gap-[0.55rem] rounded-lg border border-border-subtle px-[0.65rem] py-[0.55rem] [&_input]:mt-[0.2rem] [&_span]:grid [&_span]:gap-[0.1rem] [&_strong]:text-[0.9rem] [&_strong]:font-semibold [&_small]:text-[0.72rem]";
pub const WS_ROLE_FORM_ACTIONS: &str = "flex flex-wrap justify-end gap-[0.65rem]";

pub const WS_AUDIT_TOOLBAR: &str =
    "mb-4 mt-0 flex w-full flex-wrap items-end justify-start gap-x-4 gap-y-[0.85rem]";
pub const WS_AUDIT_FILTERS: &str =
    "flex w-full min-w-0 flex-[1_1_auto] flex-wrap items-end gap-3";
pub const WS_AUDIT_FILTER: &str =
    "grid min-w-[9.5rem] gap-[0.3rem] text-[0.85rem] font-semibold max-[720px]:min-w-0 max-[720px]:flex-[1_1_100%]";
pub const WS_AUDIT_FILTER_LABEL: &str =
    "text-[0.75rem] font-semibold uppercase tracking-[0.02em] text-secondary";
pub const WS_AUDIT_FILTER_ACTOR: &str =
    "min-w-[12rem] max-w-[18rem] flex-[1_1_12rem] max-[720px]:max-w-none max-[720px]:w-full";
pub const WS_AUDIT_HINT: &str = "mb-[1.1rem] mt-0 text-[0.85rem]";
pub const WS_AUDIT_FOOTER: &str =
    "mt-4 flex flex-wrap items-center justify-between gap-3";

pub const WS_AUDIT_COL_WHEN: &str = "w-[7.5rem]";
pub const WS_AUDIT_COL_ACTOR: &str = "w-[22%]";
pub const WS_AUDIT_COL_ACTION: &str = "w-[20%]";
pub const WS_AUDIT_COL_TARGET: &str = "w-[18%]";
pub const WS_AUDIT_COL_OUTCOME: &str = "w-[3.25rem] text-center";
pub const WS_AUDIT_COL_DETAILS: &str = "w-12 text-center";
pub const WS_AUDIT_WHEN: &str =
    "whitespace-nowrap text-[0.86rem] tabular-nums text-secondary";
pub const WS_AUDIT_ELLIPSIS: &str =
    "block max-w-full overflow-hidden text-ellipsis whitespace-nowrap text-[0.92rem] font-medium";
pub const WS_OUTCOME_ICON: &str =
    "inline-flex h-[1.7rem] w-[1.7rem] items-center justify-center rounded-full text-[0.85rem] font-bold leading-none";
pub const WS_OUTCOME_OK: &str =
    "bg-[color-mix(in_srgb,#0f7b58_16%,transparent)] text-[#0f7b58]";
pub const WS_OUTCOME_FAIL: &str =
    "bg-[color-mix(in_srgb,#dc2626_14%,transparent)] text-[#b91c1c]";
pub const WS_OUTCOME_DENY: &str =
    "bg-[color-mix(in_srgb,#d97706_16%,transparent)] text-[#b45309]";
pub const WS_OUTCOME_UNKNOWN: &str =
    "bg-[color-mix(in_srgb,#94a3b8_18%,transparent)] text-[#64748b]";

/// Audit outcome glyph chip from `outcome_class` key (`ok` / `fail` / `deny` / `unknown`).
pub fn ws_outcome_icon(kind: &str) -> String {
    let variant = match kind {
        "ok" => WS_OUTCOME_OK,
        "fail" => WS_OUTCOME_FAIL,
        "deny" => WS_OUTCOME_DENY,
        _ => WS_OUTCOME_UNKNOWN,
    };
    with_extra(WS_OUTCOME_ICON, Some(variant))
}

pub const WS_AUDIT_ICON_BUTTON: &str = "inline-flex h-8 w-8 cursor-pointer appearance-none items-center justify-center rounded-full border border-border-subtle bg-surface p-0 text-secondary transition-[background-color,border-color,color] duration-150 hover:border-border-strong hover:bg-surface-hover hover:text-primary";
pub const WS_AUDIT_EYE: &str = "block";

/// Complete audit detail dialog shell (wider than `VAULT_MODAL_CONFIRM`; do not compose widths).
pub const WS_AUDIT_DETAIL_MODAL: &str = "grid max-h-[min(84dvh,720px)] w-[min(36rem,calc(100vw-32px))] max-w-[min(36rem,96vw)] grid-rows-[auto_minmax(0,1fr)] gap-[18px] overflow-hidden rounded-[18px] border border-border-subtle bg-surface p-5 shadow-[0_24px_64px_rgba(0,0,0,0.22)]";
pub const WS_AUDIT_DETAIL_LIST: &str = "mb-[1.1rem] mt-0 grid gap-3";
pub const WS_AUDIT_DETAIL_ROW: &str = "grid gap-[0.15rem]";
pub const WS_AUDIT_DETAIL_DT: &str =
    "m-0 text-[0.72rem] font-semibold uppercase tracking-[0.03em] text-secondary";
pub const WS_AUDIT_DETAIL_DD: &str = "m-0 grid gap-[0.15rem]";
pub const WS_AUDIT_METADATA_H3: &str = "mb-1 mt-0 text-[0.95rem]";
pub const WS_AUDIT_METADATA_P: &str = "mb-2 mt-0";
pub const WS_AUDIT_JSON: &str = "mb-4 mt-0 max-h-64 overflow-auto whitespace-pre-wrap break-words rounded-lg border border-border-subtle bg-[color-mix(in_srgb,var(--text-primary)_4%,transparent)] px-[0.85rem] py-3 font-mono text-xs leading-snug";
pub const WS_MONO: &str = "font-mono text-[0.78rem]";

// ── Organizations (workspace picker + create modal) ────────────────────────

/// Organizations page root (legacy `.orgs-page`).
pub const ORG_PAGE: &str = "col-span-full grid max-w-[960px] min-w-0 gap-[22px]";
pub const ORG_TOOLBAR: &str =
    "grid items-end gap-4 grid-cols-[minmax(0,1fr)_auto] max-[720px]:grid-cols-1 max-[720px]:[&>button]:w-full";
pub const ORG_TOOLBAR_COPY: &str = "min-w-0";
/// Eyebrow / kicker — same token as `BOARD_KICKER` (legacy `.dash-eyebrow`).
pub const ORG_KICKER: &str =
    "m-0 mb-2 text-[11px] font-[650] uppercase tracking-[0.08em] text-tertiary";
pub const ORG_TOOLBAR_TITLE: &str =
    "m-0 text-[clamp(24px,2.8vw,30px)] font-[650] tracking-[-0.035em]";
pub const ORG_TOOLBAR_SUB: &str =
    "mt-2 mb-0 max-w-[52ch] text-sm leading-normal text-secondary";

pub const ORG_LIST_PANEL: &str =
    "min-w-0 overflow-hidden rounded-2xl border border-border-subtle bg-surface";
pub const ORG_LIST: &str = "m-0 grid list-none gap-0 p-0";
/// Idle workspace row (layout + hover). Do not compose with `ORG_ROW_ACTIVE` — conflicting bg/hover.
pub const ORG_ROW: &str = "grid items-center gap-3.5 border-b border-border-subtle px-4 py-3.5 transition-colors duration-150 last:border-b-0 hover:bg-surface-subtle grid-cols-[auto_minmax(0,1fr)_auto] max-[720px]:grid-cols-[auto_minmax(0,1fr)]";
/// Active tenant row — full shell (no stack with `ORG_ROW`). Hover keeps success tint (legacy `.orgs-row.is-active` won over hover).
pub const ORG_ROW_ACTIVE: &str = "grid items-center gap-3.5 border-b border-border-subtle bg-[color-mix(in_srgb,var(--success)_6%,var(--bg-surface))] px-4 py-3.5 transition-colors duration-150 last:border-b-0 hover:bg-[color-mix(in_srgb,var(--success)_6%,var(--bg-surface))] grid-cols-[auto_minmax(0,1fr)_auto] max-[720px]:grid-cols-[auto_minmax(0,1fr)]";
pub const ORG_AVATAR: &str = "inline-flex h-[42px] w-[42px] flex-none items-center justify-center rounded-xl text-[13px] font-bold tracking-wide text-white";
pub const ORG_ROW_MAIN: &str = "grid min-w-0 gap-1.5";
pub const ORG_ROW_TITLE: &str = "flex min-w-0 flex-wrap items-center gap-2 [&_strong]:max-w-full [&_strong]:truncate [&_strong]:text-sm [&_strong]:font-[650] [&_strong]:tracking-tight";
pub const ORG_ROW_META: &str = "flex flex-wrap items-center gap-2";
pub const ORG_BADGE: &str = "rounded-full border border-border-subtle bg-surface-subtle px-2 py-[3px] text-[11px] font-[650] lowercase text-secondary";
pub const ORG_BADGE_ACTIVE: &str = "rounded-full border border-[color-mix(in_srgb,var(--success)_30%,var(--border-subtle))] bg-[color-mix(in_srgb,var(--success)_14%,var(--bg-surface))] px-2 py-[3px] text-[11px] font-[650] normal-case text-success";
pub const ORG_STATUS: &str = "text-xs text-tertiary";
pub const ORG_ROW_ACTIONS: &str =
    "flex gap-2 max-[720px]:col-span-full max-[720px]:[&_a]:w-full max-[720px]:[&_button]:w-full";
pub const ORG_EMPTY: &str = "grid justify-items-center gap-2.5 px-6 py-12 text-center";
pub const ORG_EMPTY_MARK: &str = "mb-1.5 inline-flex h-[52px] w-[52px] items-center justify-center rounded-2xl bg-inverse text-lg font-bold text-on-inverse";
pub const ORG_EMPTY_TITLE: &str = "m-0 text-lg font-[650] tracking-tight";
pub const ORG_EMPTY_P: &str = "m-0 mb-2 max-w-[36ch] text-sm leading-normal text-secondary";
pub const ORG_SKELETON: &str = "grid gap-0 [&_span]:block [&_span]:h-[72px] [&_span]:rounded-xl [&_span]:border-b [&_span]:border-border-subtle [&_span]:bg-surface-subtle";

/// Create-organization modal (migrated off residual `.board-modal*` / `.orgs-create-*`).
pub const ORG_CREATE_BACKDROP: &str =
    "fixed inset-0 z-[90] grid place-items-center bg-overlay-scrim px-4 py-6 overscroll-contain";
/// Overflow visible so focus rings are not clipped (unlike shared vault/board modals).
/// Max-height matches legacy board-modal constraint; body stays overflow-visible for focus rings.
pub const ORG_CREATE_MODAL: &str = "grid max-h-[min(84dvh,720px)] w-[min(600px,100%)] max-w-[600px] gap-[18px] overflow-visible rounded-[18px] border border-border-subtle bg-surface p-5 shadow-[0_24px_64px_rgba(0,0,0,0.22)]";
pub const ORG_CREATE_BODY: &str = "grid min-h-0 overflow-visible";
pub const ORG_CREATE_FORM: &str = "grid gap-5";
pub const ORG_CREATE_FIELDS: &str = "grid gap-[18px] p-1";
pub const ORG_CREATE_ACTIONS: &str = "flex items-center justify-end gap-2.5 border-t border-border-subtle pt-[18px] max-[720px]:grid max-[720px]:grid-cols-1 max-[720px]:[&_button]:w-full";
/// Reuse vault modal head/close chrome for create dialog.
pub const ORG_CREATE_HEAD: &str = "flex items-start justify-between gap-4";
pub const ORG_CREATE_HEAD_TITLE: &str = "m-0 mb-1 text-lg font-[650] tracking-tight";
pub const ORG_CREATE_HEAD_P: &str = "m-0 text-[13px] text-secondary";
pub const ORG_CREATE_CLOSE: &str = "flex-none cursor-pointer rounded-full border border-border-subtle bg-surface-subtle px-3 py-2 text-xs font-[650] text-secondary hover:text-primary disabled:cursor-wait disabled:opacity-55";
pub const ORG_CREATE_KICKER: &str =
    "m-0 mb-[7px] text-[11px] font-[650] uppercase tracking-[0.08em] text-tertiary";

/// Org avatar with deterministic tone background (0–5).
pub fn org_avatar_class(tone: u8) -> String {
    let bg = match tone % 6 {
        0 => "bg-[#0f7b58]",
        1 => "bg-[#2563eb]",
        2 => "bg-[#a05a00]",
        3 => "bg-[#0d9488]",
        4 => "bg-[#b45309]",
        _ => "bg-[#475569]",
    };
    with_extra(ORG_AVATAR, Some(bg))
}

/// Active vs idle workspace row (standalone shells — never compose `ORG_ROW` + `ORG_ROW_ACTIVE`).
pub fn org_row_class(is_active: bool) -> &'static str {
    if is_active {
        ORG_ROW_ACTIVE
    } else {
        ORG_ROW
    }
}


// ── Workspace shell (global rail: full / mini / hidden + mobile drawer) ─────
//
// Mode variants (desktop ≥961px only — see input.css `@custom-variant`):
//   shell-mini    — [data-sidebar=mini] or html[data-sidebar-pref=mini] FOUC
//   shell-hidden  — [data-sidebar=hidden] or html[data-sidebar-pref=hidden]
//   shell-animated — .is-sidebar-animated ancestor (Cmd+B / rail toggle ~180ms)

/// Root flex shell (not grid — main must grow when rail is display:none).
pub const WS_SHELL: &str =
    "group/shell flex min-h-dvh w-full flex-row items-stretch bg-canvas shell-mobile:flex-col";

/// Checkbox peer for pure-CSS mobile drawer (no WASM required).
pub const WS_NAV_TOGGLE: &str =
    "peer absolute m-[-1px] h-px w-px overflow-hidden whitespace-nowrap border-0 p-0 [clip:rect(0,0,0,0)]";

/// Full-viewport scrim behind the mobile drawer.
pub const WS_NAV_BACKDROP: &str =
    "fixed inset-0 z-[35] m-0 hidden appearance-none border-0 bg-overlay-scrim p-0 shell-mobile:peer-checked:block";

/// Left rail — sticky desktop column; fixed off-canvas drawer on mobile.
pub const WS_SIDEBAR: &str = "sticky top-0 z-auto flex h-dvh w-[260px] max-w-[260px] min-w-0 flex-[0_0_260px] flex-col gap-4 overflow-hidden border-r border-border-subtle bg-sidebar px-2.5 py-3 shell-animated:transition-[width,flex-basis] shell-animated:duration-[180ms] shell-animated:ease-[cubic-bezier(0.16,1,0.3,1)] shell-mini:w-[68px] shell-mini:max-w-[68px] shell-mini:flex-[0_0_68px] shell-mini:items-center shell-mini:gap-3 shell-mini:!overflow-visible shell-mini:px-2 shell-mini:py-3 shell-hidden:!pointer-events-none shell-hidden:!hidden shell-hidden:!h-0 shell-hidden:!max-h-0 shell-hidden:!min-h-0 shell-hidden:!w-0 shell-hidden:!max-w-0 shell-hidden:!flex-[0_0_0] shell-hidden:!border-0 shell-hidden:!p-0 shell-hidden:!opacity-0 shell-hidden:!overflow-hidden shell-mobile:fixed shell-mobile:left-0 shell-mobile:top-0 shell-mobile:z-40 shell-mobile:h-dvh shell-mobile:w-[min(300px,86vw)] shell-mobile:max-w-[min(300px,86vw)] shell-mobile:gap-3.5 shell-mobile:overflow-y-auto shell-mobile:border-r shell-mobile:px-2.5 shell-mobile:py-3 shell-mobile:pb-3.5 shell-mobile:opacity-100 shell-mobile:pointer-events-auto shell-mobile:-translate-x-[105%] shell-mobile:transform shell-mobile:transition-transform shell-mobile:duration-200 shell-mobile:ease-[cubic-bezier(0.16,1,0.3,1)] shell-mobile:peer-checked:translate-x-0 shell-mobile:peer-checked:shadow-[8px_0_32px_rgba(0,0,0,0.14)]";

pub const WS_SIDEBAR_TOP: &str =
    "flex min-h-11 items-center justify-between gap-1 shell-mini:w-full shell-mini:flex-col shell-mini:gap-2";

pub const WS_BRAND: &str =
    "inline-flex min-w-0 flex-[1_1_auto] items-center gap-2.5 px-2 py-1.5 no-underline shell-mini:w-full shell-mini:flex-none shell-mini:justify-center shell-mini:p-1";

pub const WS_BRAND_MARK: &str =
    "inline-flex h-8 w-8 flex-none items-center justify-center rounded-[10px] bg-inverse text-sm font-bold text-on-inverse";

pub const WS_BRAND_COPY: &str =
    "min-w-0 shell-mini:hidden [&_strong]:block [&_strong]:text-[13px] [&_strong]:font-semibold [&_strong]:leading-tight [&_strong]:tracking-[-0.01em] [&_small]:mt-0.5 [&_small]:block [&_small]:text-[11px] [&_small]:leading-snug [&_small]:text-tertiary";

/// Desktop rail collapse control (full ↔ mini). Hidden on mobile drawer.
pub const WS_RAIL_TOGGLE: &str =
    "m-0 inline-flex h-8 w-8 flex-none cursor-pointer appearance-none items-center justify-center rounded-lg border-0 bg-transparent p-0 text-tertiary hover:bg-surface-hover hover:text-primary shell-mobile:!hidden";

pub const WS_RAIL_ICON: &str =
    "relative block h-[14px] w-4 rounded-[3px] border-[1.5px] border-current";

pub const WS_RAIL_ICON_BAR: &str =
    "absolute top-0 left-0 h-full w-[5px] bg-current opacity-35 shell-mini:left-auto shell-mini:right-0";

/// Mobile drawer close control (label for checkbox).
pub const WS_SIDEBAR_CLOSE: &str =
    "hidden min-h-[30px] cursor-pointer select-none appearance-none items-center rounded-lg border border-border-strong bg-transparent px-2.5 text-xs font-semibold text-secondary shell-mobile:inline-flex";

pub const WS_NAV: &str =
    "grid min-h-0 flex-[1_1_auto] content-start gap-0.5 overflow-y-auto shell-mini:w-full shell-mini:justify-items-center shell-mini:overflow-x-hidden shell-mini:overflow-y-auto shell-mobile:overflow-visible";

pub const WS_NAV_LABEL: &str =
    "mb-1 ml-2.5 mt-3 text-[11px] font-semibold uppercase tracking-[0.06em] text-tertiary shell-mini:hidden";

pub const WS_NAV_LABEL_SECONDARY: &str = "mt-4";

pub const WS_NAV_LINK: &str = "relative flex min-h-9 min-w-0 items-center gap-2.5 overflow-hidden rounded-[10px] border-0 bg-transparent px-3 py-2 text-sm font-medium leading-tight text-secondary no-underline transition-[background-color,color,padding] duration-[140ms] ease-in-out hover:bg-surface-hover hover:text-primary [&.is-active]:bg-surface-active [&.is-active]:font-semibold [&.is-active]:text-primary [&.is-disabled]:pointer-events-none [&.is-disabled]:cursor-not-allowed [&.is-disabled]:opacity-45 shell-mini:w-11 shell-mini:justify-center shell-mini:p-2.5 shell-mobile:min-h-10 shell-mobile:w-full shell-mobile:justify-start shell-mobile:px-3 shell-mobile:py-2";

pub const WS_NAV_ICON: &str =
    "inline-flex h-4 w-4 flex-none items-center justify-center text-current opacity-[0.88] [&_svg]:h-4 [&_svg]:w-4";

pub const WS_NAV_TEXT: &str =
    "min-w-0 overflow-hidden text-ellipsis whitespace-nowrap shell-mini:hidden";

pub const WS_SIDEBAR_FOOT: &str =
    "relative mt-auto border-t border-border-subtle pt-2.5 shell-mini:relative shell-mini:z-[60] shell-mini:flex shell-mini:w-full shell-mini:justify-center shell-mini:!overflow-visible shell-mobile:block";

pub const WS_MAIN: &str =
    "flex min-w-0 w-auto flex-[1_1_auto] flex-col shell-hidden:!w-full shell-hidden:!max-w-none shell-hidden:!min-w-0 shell-hidden:!flex-[1_1_100%] shell-mobile:w-full shell-mobile:flex-[1_1_auto]";

/// Sticky top chrome: mobile always; desktop only when rail is hidden (Cmd+B).
pub const WS_TOPBAR: &str =
    "sticky top-0 z-20 hidden min-h-[52px] items-center justify-start gap-2.5 border-b border-border-subtle bg-canvas px-4 shell-hidden:flex shell-mobile:flex shell-mobile:justify-start shell-mobile:px-2.5 shell-mobile:pr-3.5";

pub const WS_MENU_BUTTON_MOBILE: &str =
    "m-0 hidden h-9 w-9 flex-none cursor-pointer select-none appearance-none items-center justify-center rounded-[10px] border-0 bg-transparent p-0 text-primary order-first mr-0.5 hover:bg-surface-hover shell-mobile:inline-flex";

pub const WS_MENU_BUTTON_DESKTOP: &str =
    "m-0 hidden h-9 w-9 flex-none cursor-pointer select-none appearance-none items-center justify-center rounded-[10px] border-0 bg-transparent p-0 text-primary order-2 hover:bg-surface-hover shell-hidden:inline-flex shell-mobile:!hidden";

pub const WS_MENU_BARS: &str = "grid w-4 gap-[3.5px]";
pub const WS_MENU_BAR: &str = "block h-[1.5px] w-full rounded-full bg-primary";

pub const WS_TOPBAR_BRAND: &str =
    "hidden min-w-0 flex-[0_1_auto] items-center gap-2.5 py-0.5 no-underline order-0 shell-hidden:inline-flex shell-mobile:!hidden";

pub const WS_TOPBAR_TITLE: &str =
    "flex min-w-0 flex-[1_1_auto] items-center shell-hidden:order-1 shell-hidden:flex-[1_1_auto] shell-hidden:justify-center";

pub const WS_TOPBAR_PAGE: &str =
    "overflow-hidden text-ellipsis whitespace-nowrap text-sm font-medium text-secondary";

pub const WS_TOPBAR_ORG: &str =
    "hidden max-w-[min(240px,42vw)] min-w-0 flex-[0_1_auto] shell-mobile:ml-auto shell-mobile:block";

pub const WS_CONTENT: &str =
    "mx-auto w-full max-w-[1280px] px-7 pb-20 pt-6 shell-hidden:mx-auto shell-hidden:w-full shell-hidden:max-w-[min(1120px,100%)] shell-mobile:max-w-none shell-mobile:px-4 shell-mobile:pb-14 shell-mobile:pt-5 max-[720px]:px-4 max-[720px]:pb-16 max-[720px]:pt-5";

pub const WS_SYSTEM_NAV: &str = "contents";
pub const WS_HIDDEN_MARKER: &str = "hidden";

// ── Org switcher (sidebar foot + mobile topbar) ────────────────────────────

pub const ORG_SWITCHER: &str = "relative mb-2 w-full";
pub const ORG_SWITCHER_DETAILS: &str =
    "relative w-full [&_summary]:list-none [&_summary::-webkit-details-marker]:hidden";
pub const ORG_SWITCHER_TRIGGER: &str = "flex w-full min-h-11 cursor-pointer list-none items-center gap-2.5 rounded-xl border border-border-subtle bg-[color-mix(in_srgb,var(--bg-surface)_70%,transparent)] px-2.5 py-2 text-left hover:border-[color-mix(in_srgb,var(--accent,#6366f1)_28%,var(--border-subtle))] hover:bg-surface shell-mini:w-11 shell-mini:justify-center shell-mini:p-2 shell-mobile:w-full shell-mobile:justify-start";
pub const ORG_SWITCHER_AVATAR: &str =
    "inline-flex h-7 w-7 flex-none items-center justify-center rounded-[9px] bg-[color-mix(in_srgb,var(--accent,#6366f1)_16%,var(--bg-muted,#f4f4f5))] text-[11px] font-bold tracking-wide text-[var(--accent,#4f46e5)]";
pub const ORG_SWITCHER_META: &str =
    "flex min-w-0 flex-[1_1_auto] flex-col gap-px shell-mini:hidden";
pub const ORG_SWITCHER_LABEL: &str =
    "overflow-hidden text-ellipsis whitespace-nowrap text-[13px] font-semibold text-primary";
pub const ORG_SWITCHER_HINT: &str =
    "text-[11px] text-secondary [[data-org-placement=top]_&]:hidden";
pub const ORG_SWITCHER_CARET: &str =
    "ml-1 flex-none border-x-4 border-x-transparent border-t-[5px] border-t-secondary shell-mini:hidden";
pub const ORG_SWITCHER_PANEL: &str = "absolute bottom-[calc(100%+8px)] left-0 right-0 z-40 max-h-[min(360px,50vh)] overflow-auto rounded-xl border border-border-subtle bg-surface p-2 shadow-soft [[data-org-placement=top]_&]:bottom-auto [[data-org-placement=top]_&]:top-[calc(100%+8px)]";
pub const ORG_SWITCHER_PANEL_LABEL: &str =
    "mx-2 mb-2 mt-1 text-[11px] font-semibold uppercase tracking-[0.04em] text-secondary";
pub const ORG_SWITCHER_LIST: &str = "m-0 grid list-none gap-0.5 p-0";
pub const ORG_SWITCHER_ITEM: &str = "flex w-full cursor-pointer flex-col items-start gap-0.5 rounded-lg border-0 bg-transparent px-2.5 py-2 text-left text-inherit hover:enabled:bg-[var(--bg-muted,#f4f4f5)] disabled:cursor-default disabled:bg-[color-mix(in_srgb,var(--accent,#6366f1)_10%,transparent)] [&.is-active]:bg-[color-mix(in_srgb,var(--accent,#6366f1)_10%,transparent)] [&.is-active]:cursor-default";
pub const ORG_SWITCHER_ITEM_NAME: &str = "text-[13px] font-semibold";
pub const ORG_SWITCHER_ITEM_META: &str = "font-mono text-[11px] text-secondary";
pub const ORG_SWITCHER_DIVIDER: &str = "mx-1 my-2 h-px bg-border-subtle";
pub const ORG_SWITCHER_LINK: &str =
    "block rounded-lg px-2.5 py-2 text-[13px] text-primary no-underline hover:bg-[var(--bg-muted,#f4f4f5)]";
pub const ORG_SWITCHER_FALLBACK: &str = "p-2 text-[13px] text-secondary";

// ── Theme toggle (sidebar foot, below account flyout) ──────────────────────

pub const THEME_TOGGLE: &str = "mt-1 flex w-full min-h-11 cursor-pointer items-center gap-2.5 rounded-xl border-0 bg-transparent px-2.5 py-2 text-left hover:bg-surface-hover shell-mini:w-11 shell-mini:min-h-11 shell-mini:justify-center shell-mini:p-2 shell-mobile:w-full shell-mobile:justify-start shell-mobile:px-3 shell-mobile:py-2";
pub const THEME_TOGGLE_ICON: &str =
    "inline-flex h-[30px] w-[30px] flex-none items-center justify-center rounded-full bg-surface-subtle text-sm text-primary";
pub const THEME_TOGGLE_META: &str =
    "grid min-w-0 flex-[1_1_auto] gap-px text-left shell-mini:hidden";
pub const THEME_TOGGLE_LABEL: &str =
    "overflow-hidden text-ellipsis whitespace-nowrap text-[13px] font-medium text-primary";
pub const THEME_TOGGLE_HINT: &str = "text-[11px] text-tertiary";

// ── User menu (sidebar foot account flyout) ────────────────────────────────

pub const USER_MENU: &str = "relative w-full shell-mini:!overflow-visible";
pub const USER_MENU_DETAILS: &str =
    "group/um relative w-full shell-mini:!overflow-visible shell-mini:open:z-[200] [&_summary]:list-none [&_summary::-webkit-details-marker]:hidden";
pub const USER_MENU_TRIGGER: &str = "flex w-full min-h-12 cursor-pointer items-center gap-2.5 rounded-xl border-0 bg-transparent px-2.5 py-2 hover:bg-surface-hover group-open/um:bg-surface-active shell-mini:w-11 shell-mini:min-h-11 shell-mini:justify-center shell-mini:p-2 shell-mobile:w-full shell-mobile:justify-start shell-mobile:px-3 shell-mobile:py-2";
pub const USER_MENU_AVATAR: &str =
    "inline-flex h-[30px] w-[30px] flex-none items-center justify-center rounded-full bg-inverse text-xs font-bold text-on-inverse";
pub const USER_MENU_META: &str =
    "grid min-w-0 flex-[1_1_auto] gap-px text-left shell-mini:hidden";
pub const USER_MENU_EMAIL: &str =
    "overflow-hidden text-ellipsis whitespace-nowrap text-[13px] font-medium text-primary";
pub const USER_MENU_HINT: &str = "text-[11px] text-tertiary";
pub const USER_MENU_CARET: &str =
    "ml-1 flex-none border-x-4 border-x-transparent border-t-[5px] border-t-tertiary shell-mini:hidden";
/// Default: absolute above trigger. Mini desktop: fixed to the RIGHT of the 68px rail.
pub const USER_MENU_PANEL: &str = "absolute bottom-[calc(100%+8px)] left-0 right-0 z-50 grid gap-0.5 rounded-[14px] border border-border-subtle bg-elevated p-2 shadow-soft shell-mini:!fixed shell-mini:!bottom-3 shell-mini:!left-[76px] shell-mini:!right-auto shell-mini:!top-auto shell-mini:!z-[200] shell-mini:!w-[min(260px,calc(100vw-96px))]";
pub const USER_MENU_PANEL_LABEL: &str =
    "m-0 px-2.5 pb-1 pt-1.5 text-[11px] font-semibold uppercase tracking-[0.06em] text-tertiary";
pub const USER_MENU_ITEM: &str =
    "rounded-lg px-2.5 py-2.5 text-[13px] font-medium text-primary no-underline hover:bg-surface-hover";
pub const USER_MENU_DIVIDER: &str = "mx-1.5 my-1 h-px bg-border-subtle";
pub const USER_MENU_LOGOUT: &str = "p-0.5";
pub const USER_MENU_FALLBACK: &str = "block p-2.5 text-[13px] text-secondary no-underline";

/// Onboarding / public page brand row.
pub const PAGE_BRAND: &str = "mx-auto flex min-h-11 max-w-[1120px] items-center justify-between";
pub const PAGE_BRAND_LINK: &str =
    "inline-flex min-w-0 items-center gap-2.5 no-underline [&_strong]:block [&_strong]:text-[13px] [&_strong]:font-semibold [&_strong]:leading-tight [&_strong]:tracking-[-0.01em] [&_small]:mt-0.5 [&_small]:block [&_small]:text-[11px] [&_small]:leading-snug [&_small]:text-tertiary";
pub const PAGE_BRAND_MARK: &str =
    "inline-flex h-8 w-8 flex-none items-center justify-center rounded-[10px] bg-inverse text-sm font-bold text-on-inverse";

// ── Marketing home (public landing) ────────────────────────────────────────

pub const HOME_INTRO: &str = "min-w-0 border-t-2 border-primary py-7 pr-2 pl-0";
pub const HOME_KICKER: &str =
    "m-0 text-xs font-semibold uppercase tracking-[0.08em] text-tertiary";
pub const HOME_TITLE: &str =
    "my-[18px] max-w-[11ch] text-[clamp(28px,4.2vw,44px)] font-semibold leading-[1.04] tracking-[-0.05em]";
pub const HOME_COPY: &str = "m-0 max-w-[54ch] text-base leading-[1.65] text-secondary";
pub const HOME_ACTIONS: &str = "mt-8 flex flex-wrap items-center gap-2";
pub const HOME_STEPS: &str = "min-w-0 rounded-[14px] border border-border-subtle p-6";
pub const HOME_STEPS_LIST: &str = "m-0 mt-5 grid list-none gap-0 p-0";
pub const HOME_STEP: &str =
    "grid grid-cols-[36px_minmax(0,1fr)] gap-3.5 border-t border-border-subtle py-[18px] last:border-b last:border-border-subtle";
pub const HOME_STEP_INDEX: &str =
    "pt-0.5 font-mono text-xs text-tertiary";
pub const HOME_STEP_STRONG: &str = "block text-sm font-semibold";
pub const HOME_STEP_P: &str = "mt-[5px] mb-0 text-[13px] leading-[1.55] text-secondary";
pub const HOME_NOTE: &str = "mt-5 mb-0 text-[13px] leading-[1.55] text-tertiary";

// ── Workspace onboarding (focused first-workspace create) ──────────────────

/// Full onboarding page shell (includes former residual `.page` chrome).
pub const ONBOARDING_PAGE: &str =
    "box-border mx-auto min-h-dvh w-full max-w-[520px] bg-canvas px-5 pb-20 pt-12";
/// Complete panel chrome with onboarding gap (do not compose with `PANEL` — gap clash).
pub const ONBOARDING_CARD: &str =
    "grid min-w-0 gap-2 rounded-[14px] border border-border-subtle bg-surface p-6";
pub const ONBOARDING_TITLE: &str = "m-0 text-[1.6rem] font-[650] tracking-tight";
pub const ONBOARDING_LEDE: &str =
    "m-0 mb-4 max-w-[42ch] text-sm leading-[1.55] text-secondary";
pub const ONBOARDING_FORM: &str = "grid gap-3.5";

// ── Workspace settings shell (slug-scoped; exclusive of global rail) ───────
//
// Mobile drawer band: settings-mobile ≤900px inclusive (see input.css @custom-variant).

/// Root grid: sticky 260px rail + main. Collapses to single column ≤900px.
pub const WSS_SHELL: &str =
    "grid min-h-dvh w-full grid-cols-[260px_minmax(0,1fr)] bg-canvas text-primary settings-mobile:grid-cols-1";

/// Checkbox peer for pure-CSS mobile settings drawer.
pub const WSS_NAV_TOGGLE: &str =
    "peer absolute m-[-1px] h-px w-px overflow-hidden whitespace-nowrap border-0 p-0 [clip:rect(0,0,0,0)]";

/// Full-viewport scrim behind the mobile drawer.
pub const WSS_NAV_BACKDROP: &str =
    "fixed inset-0 z-30 m-0 hidden appearance-none border-0 bg-black/45 p-0 opacity-0 pointer-events-none transition-opacity duration-[160ms] ease-in-out settings-mobile:block settings-mobile:peer-checked:pointer-events-auto settings-mobile:peer-checked:opacity-100";

/// Settings sidebar — sticky desktop column; fixed off-canvas drawer on mobile.
pub const WSS_SIDEBAR: &str =
    "sticky top-0 flex min-h-dvh flex-col gap-2 border-r border-border-subtle bg-sidebar px-3 py-[18px] pt-4 settings-mobile:fixed settings-mobile:inset-y-0 settings-mobile:left-0 settings-mobile:z-40 settings-mobile:w-[min(320px,88vw)] settings-mobile:max-w-[min(320px,88vw)] settings-mobile:-translate-x-[105%] settings-mobile:transform settings-mobile:transition-transform settings-mobile:duration-[180ms] settings-mobile:ease-in-out settings-mobile:peer-checked:translate-x-0 settings-mobile:peer-checked:shadow-[16px_0_40px_rgba(0,0,0,0.28)]";

pub const WSS_SIDEBAR_TOP: &str =
    "flex items-start justify-between gap-2 px-1 pb-3 pt-1";

pub const WSS_IDENTITY: &str = "flex min-w-0 items-center gap-2.5";

pub const WSS_AVATAR: &str =
    "inline-flex h-9 w-9 flex-none items-center justify-center rounded-[10px] text-xs font-bold tracking-wide text-white";

pub const WSS_IDENTITY_COPY: &str = "grid min-w-0 gap-0.5 [&_strong]:overflow-hidden [&_strong]:text-ellipsis [&_strong]:whitespace-nowrap [&_strong]:text-sm [&_strong]:font-[650] [&_strong]:leading-snug [&_small]:overflow-hidden [&_small]:text-ellipsis [&_small]:whitespace-nowrap [&_small]:text-xs [&_small]:text-tertiary";

pub const WSS_SKELETON_BLOCK: &str =
    "h-9 w-9 flex-none animate-pulse rounded-lg bg-surface-subtle";
pub const WSS_SKELETON_LINE: &str =
    "block h-3 w-[120px] animate-pulse rounded-lg bg-surface-subtle";
pub const WSS_SKELETON_LINE_SM: &str =
    "mt-1.5 block h-2.5 w-[88px] animate-pulse rounded-lg bg-surface-subtle";

/// Mobile drawer close control (label for checkbox). Hidden on desktop.
pub const WSS_SIDEBAR_CLOSE: &str =
    "hidden cursor-pointer select-none appearance-none items-center rounded-lg border border-border-subtle bg-surface px-2.5 py-1.5 text-xs font-semibold text-secondary settings-mobile:inline-flex";

pub const WSS_NAV: &str =
    "grid min-h-0 flex-[1_1_auto] auto-rows-min content-start gap-0.5 overflow-y-auto";

/// Section nav link. Active/disabled via Leptos `class:is-active` / `class:is-disabled`.
pub const WSS_NAV_LINK: &str = "block rounded-lg px-2.5 py-[7px] text-[13px] font-medium leading-snug text-secondary no-underline transition-[background-color,color] duration-[140ms] ease-in-out hover:bg-surface-hover hover:text-primary [&.is-active]:bg-surface-active [&.is-active]:font-semibold [&.is-active]:text-primary [&.is-disabled]:pointer-events-none [&.is-disabled]:cursor-not-allowed [&.is-disabled]:opacity-45";

pub const WSS_SIDEBAR_FOOT: &str =
    "mt-auto grid gap-1 border-t border-border-subtle pt-3";

pub const WSS_FOOT_LINK: &str =
    "rounded-[10px] px-3 py-2 text-[13px] font-medium text-secondary no-underline hover:bg-surface-hover hover:text-primary";

pub const WSS_MAIN: &str = "flex min-w-0 flex-col";

pub const WSS_TOPBAR: &str =
    "sticky top-0 z-[2] flex items-center gap-3 border-b border-border-subtle bg-[color-mix(in_srgb,var(--bg-canvas)_88%,transparent)] px-5 py-3 backdrop-blur-[10px]";

/// Hamburger label — hidden desktop, shown ≤900px.
pub const WSS_MENU_BUTTON: &str =
    "m-0 hidden h-9 w-9 cursor-pointer select-none appearance-none items-center justify-center rounded-[10px] border border-border-subtle bg-surface p-0 settings-mobile:inline-flex";

pub const WSS_TOPBAR_TITLE: &str =
    "grid gap-0.5 [&_strong]:text-[15px] [&_strong]:font-[650]";

/// Main content column. Audit (and dense tables) widen via `data-settings-wide`.
pub const WSS_CONTENT: &str =
    "mx-auto w-full max-w-[820px] px-6 pb-16 pt-7 has-[[data-settings-wide]]:max-w-[min(1120px,100%)] settings-mobile:px-4 settings-mobile:pb-14 settings-mobile:pt-5";

/// Settings avatar with deterministic tone background (0–5).
pub fn settings_avatar_class(tone: u8) -> String {
    let bg = match tone % 6 {
        0 => "bg-[#0f7b58]",
        1 => "bg-[#2563eb]",
        2 => "bg-[#a05a00]",
        3 => "bg-[#0d9488]",
        4 => "bg-[#b45309]",
        _ => "bg-[#475569]",
    };
    with_extra(WSS_AVATAR, Some(bg))
}
