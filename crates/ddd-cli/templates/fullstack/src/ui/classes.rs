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

/// Key/value definition list parts (legacy `.kv` dt/dd).
pub const KV_DT: &str = "text-[13px] leading-normal text-secondary";
pub const KV_DD: &str = "m-0 min-w-0 break-words text-[13px] text-primary";

/// Muted helper (legacy `.board-muted`).
pub const MUTED: &str = "m-0 text-[13px] text-tertiary";

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
pub const PAGE_GRID: &str = "grid gap-4 min-w-0";

/// Error interrupt page.
pub const ERROR_PAGE: &str = "grid min-h-dvh place-items-center bg-canvas p-6";
pub const ERROR_CARD: &str =
    "w-full max-w-[480px] rounded-[14px] border border-border-subtle bg-surface p-8 shadow-soft";
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

/// Shared modal chrome (also used by org/settings via residual CSS — prefer these constants).
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
