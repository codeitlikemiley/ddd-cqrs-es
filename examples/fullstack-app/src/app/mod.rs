#![allow(unused_imports)]
#![allow(clippy::unused_unit)] // Leptos `view! {}` expands to intentional unit views.
#![allow(clippy::unit_arg)] // Empty Leptos views intentionally pass unit to `into_any`.

use crate::contracts::{
    AcceptedResponse, AccountSessionListResponse, AccountSessionSummary, AdminUserListResponse,
    AuditEventListResponse, AuthCapabilities, AuthProviderSummary,
    AuthorizationCapabilitiesResponse, CapturedMailResponse, DashboardLayout,
    DashboardNotification, DashboardSnapshot, DataSourceUpsert, EmailPasswordLoginRequest,
    EmailPasswordRegisterRequest, EmailVerificationCompleteRequest,
    EmailVerificationResendRequest, HealthStatusResponse, InvitationAcceptRequest,
    InvitationCreateRequest, InvitationListResponse, LoginCompletionResponse, LogoutResponse,
    MembershipListResponse, MfaCodeRequest, MfaEnrollConfirmResponse, MfaEnrollStartResponse,
    MfaStatusResponse, OAuthCallbackRequest, OAuthStartResponse, OrganizationCreateRequest,
    OrganizationListResponse, OrganizationSummary, PasskeyStartRequest, PasskeyStartResponse,
    PasskeyVerifyRequest, PasswordChangeRequest, PasswordResetCompleteRequest,
    PasswordResetStartRequest, PasswordResetStartResponse, PolicyPublishRequest,
    PolicyVersionListResponse, ProfileUpdateRequest, ProfileView, PublicProfileView,
    RoleListResponse, RoleUpsertRequest, SecretCreateRequest, SessionRevokeRequest, SessionView,
    SigningKeyListResponse, SigningKeyRotateRequest, SigningKeyRotateResponse,
};
use crate::ui::{
    account_page_shell, error_page_shell, page_shell, public_page_shell, AuthBrand, ErrorBanner,
    Field, FieldGroup, Panel, PrimaryButton, SectionLabel, SuccessBanner, TextInput,
};
use leptos::prelude::*;
use leptos_meta::*;
use server_fn::codec::Json;
use leptos_router::{
    components::*,
    hooks::{use_location, use_params_map},
    path,
};

#[cfg(feature = "hydrate")]
use leptos::task::spawn_local;
#[cfg(feature = "hydrate")]
use wasm_bindgen::prelude::*;
#[cfg(feature = "hydrate")]
use web_sys::window;

#[cfg(feature = "hydrate")]
#[wasm_bindgen(inline_js = r#"
function b64urlToBuffer(value) {
  const normalized = value.replace(/-/g, "+").replace(/_/g, "/");
  const padded = normalized + "===".slice((normalized.length + 3) % 4);
  const binary = atob(padded);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes.buffer;
}

function bufferToB64url(buffer) {
  const bytes = new Uint8Array(buffer);
  let binary = "";
  for (let index = 0; index < bytes.length; index += 1) {
    binary += String.fromCharCode(bytes[index]);
  }
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

function decodeCredentialDescriptors(descriptors) {
  if (!Array.isArray(descriptors)) {
    return descriptors;
  }
  return descriptors.map((descriptor) => ({
    ...descriptor,
    id: b64urlToBuffer(descriptor.id),
  }));
}

export function afterIslandHydration() {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

export function bindWorkspaceNavActive() {
  function mark() {
    window.dispatchEvent(new CustomEvent("workspace-nav-mark"));
  }
  if (window.__workspaceNavActiveBound) {
    mark();
    return;
  }
  window.__workspaceNavActiveBound = true;
  window.addEventListener("popstate", mark);
  document.addEventListener(
    "click",
    function (event) {
      const target = event.target;
      const anchor =
        target && target.closest ? target.closest("a[href]") : null;
      if (!anchor) {
        return;
      }
      const href = anchor.getAttribute("href") || "";
      if (!href.startsWith("/") || href.startsWith("//")) {
        return;
      }
      setTimeout(mark, 0);
    },
    true
  );
  const push = history.pushState.bind(history);
  const replace = history.replaceState.bind(history);
  history.pushState = function () {
    const result = push.apply(history, arguments);
    mark();
    return result;
  };
  history.replaceState = function () {
    const result = replace.apply(history, arguments);
    mark();
    return result;
  };
  mark();
}

export function initWorkspaceSidebar() {
  const shell = document.getElementById("workspace-shell");
  if (!shell || shell.dataset.sidebarReady === "1") {
    return;
  }
  shell.dataset.sidebarReady = "1";

  const MODE_KEY = "workspace-sidebar-mode";
  const EXPANDED_KEY = "workspace-sidebar-expanded";

  function isDesktop() {
    return window.matchMedia("(min-width: 961px)").matches;
  }

  function readStored(key, fallback) {
    try {
      return localStorage.getItem(key) || fallback;
    } catch (_) {
      return fallback;
    }
  }

  function writeStored(key, value) {
    try {
      localStorage.setItem(key, value);
    } catch (_) {}
  }

  function applyMode(mode, options) {
    if (mode !== "full" && mode !== "mini" && mode !== "hidden") {
      mode = "full";
    }
    const animate = !options || options.animate !== false;
    if (!animate) {
      shell.classList.remove("is-sidebar-animated");
    }
    shell.setAttribute("data-sidebar", mode);
    try {
      document.documentElement.setAttribute("data-sidebar-pref", mode);
    } catch (_) {}
    writeStored(MODE_KEY, mode);
    if (mode === "full" || mode === "mini") {
      writeStored(EXPANDED_KEY, mode);
    }

    const miniBtn = shell.querySelector('[data-sidebar-action="toggle-mini"]');
    if (miniBtn) {
      const collapsed = mode === "mini";
      miniBtn.setAttribute(
        "aria-label",
        collapsed ? "Expand sidebar" : "Collapse to mini sidebar"
      );
      miniBtn.setAttribute(
        "title",
        collapsed ? "Expand sidebar" : "Collapse to mini sidebar"
      );
      miniBtn.setAttribute("aria-pressed", collapsed ? "true" : "false");
    }

    const showBtn = shell.querySelector('[data-sidebar-action="toggle-visibility"]');
    if (showBtn) {
      showBtn.setAttribute(
        "aria-label",
        mode === "hidden" ? "Show sidebar" : "Hide sidebar"
      );
      showBtn.setAttribute(
        "title",
        mode === "hidden" ? "Show sidebar (⌘B)" : "Hide sidebar (⌘B)"
      );
    }

    // Enable width transitions only after the initial restore (user toggles).
    if (animate) {
      // Force reflow so the next class add actually transitions from the settled mode.
      void shell.offsetWidth;
      shell.classList.add("is-sidebar-animated");
    }
  }

  function restoredExpanded() {
    const expanded = readStored(EXPANDED_KEY, "full");
    return expanded === "mini" ? "mini" : "full";
  }

  function toggleMini() {
    if (!isDesktop()) {
      return;
    }
    const current = shell.getAttribute("data-sidebar") || "full";
    if (current === "hidden") {
      applyMode(restoredExpanded(), { animate: true });
      return;
    }
    applyMode(current === "mini" ? "full" : "mini", { animate: true });
  }

  function toggleVisibility() {
    if (!isDesktop()) {
      return;
    }
    const current = shell.getAttribute("data-sidebar") || "full";
    if (current === "hidden") {
      applyMode(restoredExpanded(), { animate: true });
    } else {
      applyMode("hidden", { animate: true });
    }
  }

  // Restore preferred desktop mode without animating (avoids full→mini flash on every page).
  if (isDesktop()) {
    const stored = readStored(MODE_KEY, "full");
    applyMode(
      stored === "mini" || stored === "hidden" || stored === "full" ? stored : "full",
      { animate: false }
    );
  } else {
    applyMode("full", { animate: false });
  }
  // User-driven toggles may animate from here on.
  requestAnimationFrame(function () {
    shell.classList.add("is-sidebar-animated");
  });

  shell.querySelectorAll('[data-sidebar-action="toggle-mini"]').forEach((btn) => {
    btn.addEventListener("click", (event) => {
      event.preventDefault();
      event.stopPropagation();
      toggleMini();
    });
  });

  shell.querySelectorAll('[data-sidebar-action="toggle-visibility"]').forEach((btn) => {
    btn.addEventListener("click", (event) => {
      event.preventDefault();
      event.stopPropagation();
      toggleVisibility();
    });
  });

  window.addEventListener("keydown", (event) => {
    if (!(event.metaKey || event.ctrlKey)) {
      return;
    }
    if (String(event.key || "").toLowerCase() !== "b") {
      return;
    }
    const target = event.target;
    if (
      target &&
      (target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.tagName === "SELECT" ||
        target.isContentEditable)
    ) {
      return;
    }
    if (!isDesktop()) {
      return;
    }
    event.preventDefault();
    toggleVisibility();
  });

  window.matchMedia("(min-width: 961px)").addEventListener("change", (event) => {
    if (!event.matches) {
      // Mobile always uses the drawer; keep full markup widths.
      applyMode("full", { animate: false });
    } else {
      const stored = readStored(MODE_KEY, "full");
      applyMode(
        stored === "mini" || stored === "hidden" || stored === "full" ? stored : "full",
        { animate: false }
      );
    }
  });

  bindUserMenuDismiss();
}

export function bindUserMenuDismiss() {
  if (window.__userMenuDismissBound) {
    return;
  }
  window.__userMenuDismissBound = true;

  function closeOpenMenus(except) {
    document.querySelectorAll(".user-menu-details[open]").forEach((details) => {
      if (except && details === except) {
        return;
      }
      details.removeAttribute("open");
    });
  }

  document.addEventListener(
    "pointerdown",
    function (event) {
      const target = event.target;
      if (!target || !target.closest) {
        return;
      }
      const open = document.querySelector(".user-menu-details[open]");
      if (!open) {
        return;
      }
      if (open.contains(target)) {
        return;
      }
      closeOpenMenus(null);
    },
    true
  );

  document.addEventListener("keydown", function (event) {
    if (event.key === "Escape") {
      closeOpenMenus(null);
    }
  });
}

export function pickImageDataUrl(input, maxBytes) {
  return new Promise(function (resolve, reject) {
    const file = input && input.files && input.files[0];
    if (!file) {
      resolve(null);
      return;
    }
    if (maxBytes && file.size > maxBytes) {
      reject(new Error("Image is too large. Use a file under 250 KB."));
      return;
    }
    if (!String(file.type || "").startsWith("image/")) {
      reject(new Error("Choose a PNG, JPEG, WebP, or GIF image."));
      return;
    }
    const reader = new FileReader();
    reader.onload = function () {
      resolve(reader.result);
    };
    reader.onerror = function () {
      reject(reader.error || new Error("Could not read image."));
    };
    reader.readAsDataURL(file);
  });
}

export function passkeySupported() {
  return Boolean(window.PublicKeyCredential && navigator.credentials);
}

function passkeyLoopbackHint() {
  const host = window.location.hostname;
  if (host === "127.0.0.1" || host === "::1" || host === "[::1]") {
    const port = window.location.port ? ":" + window.location.port : "";
    return (
      " Browsers reject IP addresses as WebAuthn rpId — open http://localhost" +
      port +
      window.location.pathname +
      " (not 127.0.0.1) and set AUTH_PASSKEY_RP_ID=localhost with AUTH_PASSKEY_ORIGIN matching that URL."
    );
  }
  return "";
}

function passkeyErrorMessage(error) {
  if (!error) {
    return "Passkey prompt was cancelled or unavailable.";
  }
  const name = error.name || "Error";
  const message = error.message || String(error);
  // User dismissed the OS/browser sheet, timed out, or no authenticator responded.
  if (name === "NotAllowedError" || name === "AbortError") {
    return "Passkey prompt was cancelled.";
  }
  if (name === "InvalidStateError") {
    return "A passkey for this account may already exist on this device.";
  }
  if (name === "SecurityError") {
    return (
      "Passkey blocked by browser security policy. Open the app on the exact origin configured for WebAuthn (rpId/origin must match)." +
      passkeyLoopbackHint()
    );
  }
  if (name === "NotSupportedError") {
    return "This browser does not support the requested passkey options.";
  }
  return message || name + ": " + String(error);
}

function preparePublicKeyOptions(optionsJson) {
  const publicKey = JSON.parse(optionsJson);
  publicKey.challenge = b64urlToBuffer(publicKey.challenge);
  if (publicKey.user && publicKey.user.id) {
    publicKey.user.id = b64urlToBuffer(publicKey.user.id);
  }
  publicKey.excludeCredentials = decodeCredentialDescriptors(publicKey.excludeCredentials);
  publicKey.allowCredentials = decodeCredentialDescriptors(publicKey.allowCredentials);
  return publicKey;
}

function serializeCreatedCredential(credential) {
  if (!credential) {
    throw new Error("No passkey credential was created.");
  }
  const transports = credential.response.getTransports
    ? credential.response.getTransports()
    : [];
  return JSON.stringify({
    id: bufferToB64url(credential.rawId),
    transports,
    attestationObject: bufferToB64url(credential.response.attestationObject),
    clientDataJSON: bufferToB64url(credential.response.clientDataJSON),
  });
}

export async function createPasskeyCredential(optionsJson) {
  const loopbackHint = passkeyLoopbackHint();
  if (loopbackHint) {
    throw loopbackHint.trim();
  }
  const publicKey = preparePublicKeyOptions(optionsJson);
  try {
    const credential = await navigator.credentials.create({ publicKey });
    return serializeCreatedCredential(credential);
  } catch (error) {
    // Platform-only often fails instantly on desktops without Touch ID / Hello.
    // Retry once without attachment so security keys / hybrid passkeys work.
    const attachment =
      publicKey.authenticatorSelection &&
      publicKey.authenticatorSelection.authenticatorAttachment;
    if (
      error &&
      error.name === "NotAllowedError" &&
      attachment &&
      attachment !== "undefined"
    ) {
      try {
        const retryKey = { ...publicKey };
        retryKey.authenticatorSelection = {
          ...publicKey.authenticatorSelection,
        };
        delete retryKey.authenticatorSelection.authenticatorAttachment;
        const credential = await navigator.credentials.create({
          publicKey: retryKey,
        });
        return serializeCreatedCredential(credential);
      } catch (retryError) {
        // Throw a plain string so wasm-bindgen surfaces a clean message
        // instead of `JsValue(Error: … at createPasskeyCredential …)`.
        throw passkeyErrorMessage(retryError);
      }
    }
    throw passkeyErrorMessage(error);
  }
}

export async function isConditionalMediationAvailable() {
  try {
    return Boolean(
      window.PublicKeyCredential &&
        PublicKeyCredential.isConditionalMediationAvailable &&
        (await PublicKeyCredential.isConditionalMediationAvailable())
    );
  } catch (_) {
    return false;
  }
}

export async function getPasskeyCredential(optionsJson, mediation) {
  const loopbackHint = passkeyLoopbackHint();
  if (loopbackHint) {
    throw loopbackHint.trim();
  }
  const publicKey = preparePublicKeyOptions(optionsJson);
  const request = { publicKey };
  if (
    mediation === "conditional" ||
    mediation === "optional" ||
    mediation === "required"
  ) {
    request.mediation = mediation;
  }
  try {
    const credential = await navigator.credentials.get(request);
    if (!credential) {
      throw new Error("No passkey credential was selected.");
    }
    const response = {
      id: bufferToB64url(credential.rawId),
      authenticatorData: bufferToB64url(credential.response.authenticatorData),
      signature: bufferToB64url(credential.response.signature),
      clientDataJSON: bufferToB64url(credential.response.clientDataJSON),
    };
    if (credential.response.userHandle) {
      response.userHandle = bufferToB64url(credential.response.userHandle);
    }
    return JSON.stringify(response);
  } catch (error) {
    // Conditional UI stays open until the user picks a passkey, submits another
    // method, or navigates away. Treat cancel/idle as a soft no-op.
    if (
      mediation === "conditional" &&
      error &&
      (error.name === "AbortError" || error.name === "NotAllowedError")
    ) {
      throw "PASSKEY_CONDITIONAL_IDLE";
    }
    throw passkeyErrorMessage(error);
  }
}

export async function copyText(value) {
  if (navigator.clipboard && navigator.clipboard.writeText) {
    await navigator.clipboard.writeText(value);
    return true;
  }
  const area = document.createElement("textarea");
  area.value = value;
  area.setAttribute("readonly", "");
  area.style.position = "fixed";
  area.style.left = "-9999px";
  document.body.appendChild(area);
  area.select();
  const ok = document.execCommand("copy");
  document.body.removeChild(area);
  return ok;
}
"#)]
extern "C" {
    #[wasm_bindgen(catch, js_name = afterIslandHydration)]
    pub(crate) async fn after_island_hydration() -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_name = initWorkspaceSidebar)]
    pub(crate) fn init_workspace_sidebar();

    #[wasm_bindgen(js_name = bindWorkspaceNavActive)]
    pub(crate) fn bind_workspace_nav_active();

    #[wasm_bindgen(js_name = bindUserMenuDismiss)]
    pub(crate) fn bind_user_menu_dismiss();

    #[wasm_bindgen(catch, js_name = pickImageDataUrl)]
    pub(crate) async fn pick_image_data_url(
        input: web_sys::HtmlInputElement,
        max_bytes: u32,
    ) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_name = passkeySupported)]
    pub(crate) fn passkey_supported() -> bool;

    #[wasm_bindgen(catch, js_name = isConditionalMediationAvailable)]
    pub(crate) async fn is_conditional_mediation_available() -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, js_name = copyText)]
    pub(crate) async fn copy_text(value: String) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, js_name = createPasskeyCredential)]
    pub(crate) async fn create_passkey_credential(options_json: String) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, js_name = getPasskeyCredential)]
    pub(crate) async fn get_passkey_credential(
        options_json: String,
        mediation: String,
    ) -> Result<JsValue, JsValue>;
}

pub mod account;
pub mod auth;
pub mod dashboard;
pub mod helpers;
pub mod path;
pub mod router;
pub mod server_fns;
pub mod workspace;

pub use account::*;
pub use auth::*;
pub use dashboard::{DashboardHome, DashboardPage};
pub use helpers::*;
pub use path::*;
pub use router::{shell, App};
pub use server_fns::*;
pub use workspace::{
    AppLayout, WorkspaceOnboardingGate, WorkspaceOnboardingPage, WorkspaceShell,
};

use crate::app::helpers::{
    action_result_text, current_browser_pathname, has_permission, next_url, optional_text,
    org_monogram, org_tone_index, redirect_browser, selected_action_error, selected_auth_error,
    server_error_text, short_id_label, validate_email_only, validate_login_form,
};
use crate::app::path::{is_workspace_path, workspace_topbar_title};


#[component]
pub fn HomePage() -> impl IntoView {
    public_page_shell(
        "Production fullstack Rust",
        "Leptos islands, trusted authentication, embedded Cedar, DDD persistence, REST, and Spin gRPC in one component.",
        view! {
            <section class="home-intro">
                <p class="home-kicker">"A calm starting point for a serious system"</p>
                <h2>"One verified session for every surface."</h2>
                <p class="home-copy">
                    "Create an account, verify your email, then move between the Leptos UI, REST endpoints, and authenticated gRPC services without changing the application boundary."
                </p>
                <div class="actions">
                    <a class="link-button link-button-primary" href="/register">"Create account"</a>
                    <a class="link-button" href="/login">"Sign in"</a>
                </div>
            </section>
            <section class="home-steps">
                <p class="section-label">"Start here"</p>
                <ol class="steps-list">
                    <li>
                        <span class="step-index">"01"</span>
                        <div><strong>"Register"</strong><p>"Create a global account with a password or an enabled provider."</p></div>
                    </li>
                    <li>
                        <span class="step-index">"02"</span>
                        <div><strong>"Verify"</strong><p>"Use the one-time link delivered by the configured mail transport."</p></div>
                    </li>
                    <li>
                        <span class="step-index">"03"</span>
                        <div><strong>"Organize"</strong><p>"Create or join a tenant, then manage access from the protected workspace."</p></div>
                    </li>
                </ol>
                <p class="home-note">"The browser shell stays server-rendered. Only interactive forms and protected controls hydrate."</p>
            </section>
        },
    )
}

#[component]
pub fn AuthProviderAdminPage() -> impl IntoView {
    page_shell(
        "Auth providers",
        "Configure OAuth and OIDC providers.",
        view! { <ProviderConfigForm /> },
    )
}

#[component]
pub fn SigningKeyAdminPage() -> impl IntoView {
    page_shell(
        "Signing keys",
        "Rotate the active access-token signing key.",
        view! { <SigningKeyRotationForm /> },
    )
}

#[component]
pub fn RedirectAllowlistPage() -> impl IntoView {
    page_shell(
        "Redirect allowlist",
        "Restrict browser redirect targets.",
        view! { <RedirectAllowlistForm /> },
    )
}

#[island(lazy)]
pub fn AuthorizationPolicyPage() -> impl IntoView {
    let capabilities = browser_load(get_authorization_capabilities);
    page_shell(
        "Authorization policy",
        "Inspect the active embedded Cedar provider. Policy publication is restricted to MFA-authenticated system administrators.",
        view! {
            <section class="panel">
                <h2>"Active provider"</h2>
                <div class="client-data-slot">
                    {move || capabilities.get().map(|result| match result {
                        Ok(value) => view! {
                            <dl class="kv">
                                <dt>"Provider"</dt><dd>{value.provider}</dd>
                                <dt>"Maximum batch"</dt><dd>{value.max_batch_checks}</dd>
                                <dt>"Resource listing"</dt><dd>{value.list_resources}</dd>
                                <dt>"Consistency tokens"</dt><dd>{value.consistency_tokens}</dd>
                            </dl>
                        }.into_any(),
                        Err(error) => view! {
                            <p class="result-line">{server_error_text(error)}</p>
                        }.into_any(),
                    })}
                </div>
            </section>
        },
    )
}



/// Best-effort async sleep for hydrate (vault reveal auto-mask).
#[cfg(feature = "hydrate")]
async fn gloo_timers_sleep_ms(ms: u64) {
    use wasm_bindgen_futures::JsFuture;
    use js_sys::Promise;
    let promise = Promise::new(&mut |resolve, _reject| {
        if let Some(window) = web_sys::window() {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                &resolve,
                ms as i32,
            );
        } else {
            let _ = resolve.call0(&wasm_bindgen::JsValue::NULL);
        }
    });
    let _ = JsFuture::from(promise).await;
}

#[cfg(not(feature = "hydrate"))]
async fn gloo_timers_sleep_ms(_ms: u64) {}

#[island(lazy)]
pub fn OrganizationsPage() -> impl IntoView {
    page_shell(
        "Organizations",
        "Workspaces you belong to. Select one to scope members, roles, and audit.",
        view! { <OrganizationsHome /> },
    )
}

#[island]
fn OrganizationsHome() -> impl IntoView {
    let organizations = browser_load(list_organizations);
    let session = browser_load(get_current_session);
    let create_action = ServerAction::<CreateOrganization>::new();
    let create_pending = create_action.pending();
    let create_value = create_action.value();
    let select_action = ServerAction::<SelectOrganization>::new();
    let select_pending = select_action.pending();
    let select_value = select_action.value();
    let (name, set_name) = signal(String::new());
    let (slug, set_slug) = signal(String::new());
    let (slug_touched, set_slug_touched) = signal(false);
    let (create_open, set_create_open) = signal(false);

    let derive_slug = |raw: &str| -> String {
        let mut out = String::new();
        let mut prev_dash = false;
        for ch in raw.trim().chars() {
            let lower = ch.to_ascii_lowercase();
            if lower.is_ascii_alphanumeric() {
                out.push(lower);
                prev_dash = false;
            } else if !prev_dash && !out.is_empty() {
                out.push('-');
                prev_dash = true;
            }
        }
        out.trim_matches('-').chars().take(48).collect()
    };

    Effect::new(move |_| {
        if matches!(create_value.get(), Some(Ok(_))) {
            set_name.set(String::new());
            set_slug.set(String::new());
            set_slug_touched.set(false);
            set_create_open.set(false);
            #[cfg(feature = "hydrate")]
            {
                // Refresh the list after a successful create.
                if let Some(window) = window() {
                    let _ = window.location().reload();
                }
            }
        }
    });

    Effect::new(move |_| {
        if matches!(select_value.get(), Some(Ok(_))) {
            #[cfg(feature = "hydrate")]
            {
                if let Some(window) = window() {
                    let _ = window.location().reload();
                }
            }
        }
    });

    view! {
        <div class="orgs-page">
            <header class="orgs-toolbar">
                <div class="orgs-toolbar-copy">
                    <p class="dash-eyebrow">"Tenancy"</p>
                    <h2 class="orgs-toolbar-title">"Your workspaces"</h2>
                    <p class="orgs-toolbar-sub">
                        "The first workspace is the default after sign-in. Select another to switch the active tenant."
                    </p>
                </div>
                <button
                    type="button"
                    class="primary-button"
                    on:click=move |_| set_create_open.update(|open| *open = !*open)
                >
                    {move || if create_open.get() { "Close" } else { "New organization" }}
                </button>
            </header>

            <Show when=move || create_open.get()>
                <section class="orgs-create panel" aria-label="Create organization">
                    <div class="orgs-create-head">
                        <h3>"Create organization"</h3>
                        <p>"Name it something your team will recognize. You become the owner."</p>
                    </div>
                    <div class="orgs-create-form">
                        <label class="auth-field">
                            <span>"Organization name"</span>
                            <input
                                class="auth-input"
                                type="text"
                                maxlength="120"
                                placeholder="Northwind Studio"
                                prop:value=move || name.get()
                                on:input=move |event| {
                                    let v = event_target_value(&event);
                                    set_name.set(v.clone());
                                    if !slug_touched.get_untracked() {
                                        set_slug.set(derive_slug(&v));
                                    }
                                }
                            />
                        </label>
                        <label class="auth-field">
                            <span>"Workspace URL"</span>
                            <div class="onboarding-slug-row">
                                <span class="onboarding-slug-prefix">"/org/"</span>
                                <input
                                    class="auth-input mono-value"
                                    type="text"
                                    maxlength="48"
                                    placeholder="northwind"
                                    prop:value=move || slug.get()
                                    on:input=move |event| {
                                        set_slug_touched.set(true);
                                        set_slug.set(derive_slug(&event_target_value(&event)));
                                    }
                                />
                            </div>
                        </label>
                        <button
                            type="button"
                            class="primary-button"
                            disabled=move || {
                                create_pending.get()
                                    || name.get().trim().is_empty()
                                    || slug.get().trim().len() < 2
                            }
                            on:click=move |_| {
                                let value = name.get_untracked().trim().to_owned();
                                let slug_value = slug.get_untracked().trim().to_owned();
                                if value.is_empty() || slug_value.len() < 2 {
                                    return;
                                }
                                create_action.dispatch(CreateOrganization {
                                    name: value,
                                    slug: slug_value,
                                });
                            }
                        >
                            {move || {
                                if create_pending.get() {
                                    "Creating…"
                                } else {
                                    "Create organization"
                                }
                            }}
                        </button>
                    </div>
                    <Show when=move || {
                        create_value.get().is_some_and(|result| result.is_err())
                    }>
                        <p class="error-banner">{move || action_result_text(create_value.get())}</p>
                    </Show>
                </section>
            </Show>

            <Show when=move || select_value.get().is_some_and(|result| result.is_err())>
                <p class="error-banner">{move || action_result_text(select_value.get())}</p>
            </Show>

            {move || match session.get() {
                Some(Ok(session)) if session.authenticated => {
                    let can_settings = has_permission(&session, "organization.update");
                    let can_members = has_permission(&session, "member.view");
                    let can_roles = has_permission(&session, "role.view");
                    let can_audit = has_permission(&session, "audit.view");
                    let active_tenant = session
                        .tenant_id
                        .clone()
                        .filter(|value| !value.trim().is_empty());
                    if can_settings || can_members || can_roles || can_audit {
                        view! {
                            <nav class="orgs-context-nav" aria-label="Selected organization">
                                <div class="orgs-context-label">
                                    <span class="dash-metric-dot dash-metric-dot-ok" aria-hidden="true"></span>
                                    <span>
                                        {match active_tenant.as_ref() {
                                            Some(id) => format!("Active · {}", short_id_label(id)),
                                            None => "Select an organization to unlock management".to_owned(),
                                        }}
                                    </span>
                                </div>
                                <div class="orgs-context-links">
                                    <a class="orgs-context-link" href="/account/vault">"Vault"</a>
                                    <Show when=move || can_settings>
                                        <a class="orgs-context-link" href="/organizations/settings">"Settings"</a>
                                    </Show>
                                    <Show when=move || can_members>
                                        <a class="orgs-context-link" href="/organizations/members">"Members"</a>
                                        <a class="orgs-context-link" href="/organizations/invitations">"Invitations"</a>
                                    </Show>
                                    <Show when=move || can_roles>
                                        <a class="orgs-context-link" href="/organizations/roles">"Roles"</a>
                                        <a class="orgs-context-link" href="/organizations/permissions">"Permissions"</a>
                                    </Show>
                                    <Show when=move || can_audit>
                                        <a class="orgs-context-link" href="/organizations/audit">"Audit"</a>
                                    </Show>
                                </div>
                            </nav>
                        }.into_any()
                    } else {
                        view! {}.into_any()
                    }
                }
                _ => view! {}.into_any(),
            }}

            <section class="orgs-list-panel" aria-label="Organization list">
                {move || {
                    let active_tenant = session
                        .get()
                        .and_then(Result::ok)
                        .and_then(|s| s.tenant_id)
                        .filter(|value| !value.trim().is_empty());
                    match organizations.get() {
                        Some(Ok(response)) if response.organizations.is_empty() => view! {
                            <div class="orgs-empty">
                                <div class="orgs-empty-mark" aria-hidden="true">"O"</div>
                                <h3>"No organizations yet"</h3>
                                <p>"Create your first workspace to invite teammates and manage roles."</p>
                                <button
                                    type="button"
                                    class="primary-button"
                                    on:click=move |_| set_create_open.set(true)
                                >
                                    "Create organization"
                                </button>
                            </div>
                        }.into_any(),
                        Some(Ok(response)) => view! {
                            <ul class="orgs-list">
                                <For
                                    each=move || response.organizations.clone()
                                    key=|organization| organization.organization_id.clone()
                                    children=move |organization| {
                                        let organization_id = organization.organization_id.clone();
                                        let select_id = organization_id.clone();
                                        let org_slug = organization.slug.clone();
                                        let is_active = active_tenant
                                            .as_ref()
                                            .is_some_and(|id| id == &organization_id);
                                        let monogram = org_monogram(&organization.name);
                                        let tone = org_tone_index(&organization.name);
                                        let role = organization.current_user_role.clone();
                                        let status = organization.status.clone();
                                        let vault_href = if org_slug.is_empty() {
                                            "/account/vault".to_owned()
                                        } else {
                                            format!("/org/{org_slug}/vault")
                                        };
                                        let action = if is_active {
                                            view! {
                                                <a class="secondary-button" href=vault_href.clone()>"Vault"</a>
                                                <a class="secondary-button" href="/organizations/settings">
                                                    "Open"
                                                </a>
                                            }
                                            .into_any()
                                        } else {
                                            view! {
                                                <a class="secondary-button" href=vault_href.clone()>"Vault"</a>
                                                <button
                                                    type="button"
                                                    class="secondary-button"
                                                    disabled=move || select_pending.get()
                                                    on:click=move |_| {
                                                        select_action.dispatch(SelectOrganization {
                                                            organization_id: select_id.clone(),
                                                        });
                                                    }
                                                >
                                                    "Select"
                                                </button>
                                            }
                                            .into_any()
                                        };
                                        view! {
                                            <li
                                                class="orgs-row"
                                                class:is-active=is_active
                                            >
                                                <div
                                                    class="orgs-avatar"
                                                    data-tone=tone.to_string()
                                                    aria-hidden="true"
                                                >
                                                    {monogram}
                                                </div>
                                                <div class="orgs-row-main">
                                                    <div class="orgs-row-title">
                                                        <strong>{organization.name.clone()}</strong>
                                                        {if is_active {
                                                            view! {
                                                                <span class="orgs-badge orgs-badge-active">"Active"</span>
                                                            }
                                                            .into_any()
                                                        } else {
                                                            view! {}.into_any()
                                                        }}
                                                    </div>
                                                    <div class="orgs-row-meta">
                                                        <span class="orgs-badge">{role}</span>
                                                        <span class="orgs-status">{status}</span>
                                                    </div>
                                                </div>
                                                <div class="orgs-row-actions">
                                                    {action}
                                                </div>
                                            </li>
                                        }
                                    }
                                />
                            </ul>
                        }.into_any(),
                        Some(Err(error)) => view! {
                            <p class="error-banner">{server_error_text(error)}</p>
                        }.into_any(),
                        None => view! {
                            <div class="orgs-skeleton" aria-busy="true">
                                <span></span><span></span><span></span>
                            </div>
                        }.into_any(),
                    }
                }}
                <Show when=move || {
                    select_value.get().is_some_and(|result| result.is_err())
                }>
                    <p class="error-banner">{move || action_result_text(select_value.get())}</p>
                </Show>
            </section>
        </div>
    }
}

#[component]
pub fn OrganizationSettingsPage() -> impl IntoView {
    page_shell(
        "Organization settings",
        "The selected tenant comes from the verified session, never from an untrusted form alone.",
        view! { <SessionSummary /> <OrganizationLinks /> },
    )
}

#[island(lazy)]
pub fn OrganizationMembersPage() -> impl IntoView {
    let members = browser_load(list_current_organization_members);
    page_shell(
        "Members",
        "Review active, blocked, and removed organization memberships.",
        view! {
            <section class="panel">
                <h2>"Memberships"</h2>
                <div class="client-data-slot">
                    {move || match members.get() {
                        Some(Ok(response)) => view! {
                            <dl class="kv">
                                <For
                                    each=move || response.memberships.clone()
                                    key=|membership| membership.user_id.clone()
                                    children=move |membership| view! {
                                        <dt>{membership.primary_email}</dt>
                                        <dd>{format!("{} / {}", membership.role_id, membership.status)}</dd>
                                    }
                                />
                            </dl>
                        }.into_any(),
                        Some(Err(error)) => view! { <p class="error-banner">{server_error_text(error)}</p> }.into_any(),
                        None => view! { <p class="result-line">"Loading members"</p> }.into_any(),
                    }}
                </div>
            </section>
            <OrganizationLinks />
        },
    )
}

#[island(lazy)]
pub fn OrganizationInvitationsPage() -> impl IntoView {
    let invitations = browser_load(list_current_organization_invitations);
    let invite_action = ServerAction::<InviteCurrentOrganizationMember>::new();
    let invite_pending = invite_action.pending();
    let invite_value = invite_action.value();
    let (email, set_email) = signal(String::new());
    let (role_id, set_role_id) = signal("member".to_owned());
    page_shell(
        "Invitations",
        "One-time invitation values are mailed and only their hashes are persisted.",
        view! {
            <section class="panel">
                <h2>"Invite member"</h2>
                <label><span>"Email"</span><input type="email" prop:value=move || email.get() on:input=move |event| set_email.set(event_target_value(&event)) /></label>
                <label><span>"Role"</span><input type="text" prop:value=move || role_id.get() on:input=move |event| set_role_id.set(event_target_value(&event)) /></label>
                <button type="button" class="primary-button" disabled=move || invite_pending.get() on:click=move |_| {
                    invite_action.dispatch(InviteCurrentOrganizationMember {
                        email: email.get_untracked(),
                        role_id: role_id.get_untracked(),
                    });
                }>"Send invitation"</button>
                <Show when=move || invite_value.get().is_some()><p class="result-line">{move || action_result_text(invite_value.get())}</p></Show>
            </section>
            <section class="panel">
                <h2>"Invitation status"</h2>
                <div class="client-data-slot">
                    {move || match invitations.get() {
                        Some(Ok(response)) => view! {
                            <dl class="kv"><For each=move || response.invitations.clone() key=|invitation| invitation.invitation_id.clone() children=move |invitation| view! {
                                <dt>{invitation.email}</dt><dd>{format!("{} / {}", invitation.role_id, invitation.status)}</dd>
                            } /></dl>
                        }.into_any(),
                        Some(Err(error)) => view! { <p class="error-banner">{server_error_text(error)}</p> }.into_any(),
                        None => view! { <p class="result-line">"Loading invitations"</p> }.into_any(),
                    }}
                </div>
            </section>
        },
    )
}

#[island(lazy)]
pub fn OrganizationRolesPage() -> impl IntoView {
    let roles = browser_load(list_current_organization_roles);
    let upsert_action = ServerAction::<UpsertCurrentOrganizationRole>::new();
    let (role_id, set_role_id) = signal(String::new());
    let (name, set_name) = signal(String::new());
    let (permissions, set_permissions) = signal("organization.view,counter.view".to_owned());
    page_shell(
        "Roles",
        "Built-in roles are immutable; custom roles use the bounded tenant permission catalog.",
        view! {
            <section class="panel">
                <h2>"Role catalog"</h2>
                <div class="client-data-slot">
                    {move || match roles.get() {
                        Some(Ok(response)) => view! { <dl class="kv"><For each=move || response.roles.clone() key=|role| role.role_id.clone() children=move |role| view! {
                            <dt>{role.name}</dt><dd>{format!("{} permissions{}", role.permissions.len(), if role.built_in { " / built-in" } else { "" })}</dd>
                        } /></dl> }.into_any(),
                        Some(Err(error)) => view! { <p class="error-banner">{server_error_text(error)}</p> }.into_any(),
                        None => view! { <p class="result-line">"Loading roles"</p> }.into_any(),
                    }}
                </div>
            </section>
            <section class="panel">
                <h2>"Custom role"</h2>
                <label><span>"Role id"</span><input type="text" prop:value=move || role_id.get() on:input=move |event| set_role_id.set(event_target_value(&event)) /></label>
                <label><span>"Name"</span><input type="text" prop:value=move || name.get() on:input=move |event| set_name.set(event_target_value(&event)) /></label>
                <label><span>"Comma-separated permissions"</span><input type="text" prop:value=move || permissions.get() on:input=move |event| set_permissions.set(event_target_value(&event)) /></label>
                <button type="button" class="secondary-button" on:click=move |_| {
                    upsert_action.dispatch(UpsertCurrentOrganizationRole {
                        role_id: role_id.get_untracked(),
                        name: name.get_untracked(),
                        permissions: permissions.get_untracked().split(',').map(str::trim).filter(|value| !value.is_empty()).map(ToOwned::to_owned).collect(),
                    });
                }>"Save role"</button>
                <Show when=move || upsert_action.value().get().is_some()><p class="result-line">{move || action_result_text(upsert_action.value().get())}</p></Show>
            </section>
        },
    )
}

#[island(lazy)]
pub fn OrganizationPermissionsPage() -> impl IntoView {
    let roles = browser_load(list_current_organization_roles);
    page_shell(
        "Permissions",
        "Inspect effective permission assignments through organization roles.",
        view! {
            <section class="panel">
                <div class="client-data-slot">
                    {move || match roles.get() {
                        Some(Ok(response)) => view! { <div class="action-stack"><For each=move || response.roles.clone() key=|role| role.role_id.clone() children=move |role| view! {
                            <article class="compact-panel"><h3>{role.name}</h3><p class="result-line">{role.permissions.join(", ")}</p></article>
                        } /></div> }.into_any(),
                        Some(Err(error)) => view! { <p class="error-banner">{server_error_text(error)}</p> }.into_any(),
                        None => view! { <p class="result-line">"Loading permissions"</p> }.into_any(),
                    }}
                </div>
            </section>
        },
    )
}

#[island(lazy)]
pub fn OrganizationAuditPage() -> impl IntoView {
    let audit = browser_load(list_current_organization_audit);
    page_shell(
        "Audit activity",
        "Cursor-based audit reads share the same authorization path as the gRPC server stream.",
        view! {
            <section class="panel"><div class="client-data-slot">
                {move || match audit.get() {
                    Some(Ok(response)) => view! { <dl class="kv"><For each=move || response.events.clone() key=|event| event.sequence children=move |event| view! {
                        <dt>{event.action}</dt><dd>{format!("{}:{} / {}", event.target_type, event.target_id, event.outcome)}</dd>
                    } /></dl> }.into_any(),
                    Some(Err(error)) => view! { <p class="error-banner">{server_error_text(error)}</p> }.into_any(),
                    None => view! { <p class="result-line">"Loading audit activity"</p> }.into_any(),
                }}
            </div></section>
        },
    )
}

#[component]
fn OrganizationLinks() -> impl IntoView {
    view! {
        <section class="panel panel-inline">
            <a class="text-link" href="/organizations">"Back to organizations"</a>
        </section>
    }
}

#[island(lazy)]
pub fn AdminUsersPage() -> impl IntoView {
    let users = browser_load(list_admin_users);
    page_shell(
        "System users",
        "Disable or recover users without deleting immutable audit history.",
        view! { <section class="panel"><div class="client-data-slot">
            {move || match users.get() {
                Some(Ok(response)) => view! { <dl class="kv"><For each=move || response.users.clone() key=|user| user.user_id.clone() children=move |user| view! {
                    <dt>{user.primary_email}</dt><dd>{if user.disabled { "disabled" } else if user.email_verified { "active / verified" } else { "pending verification" }}</dd>
                } /></dl> }.into_any(),
                Some(Err(error)) => view! { <p class="error-banner">{server_error_text(error)}</p> }.into_any(),
                None => view! { <p class="result-line">"Loading users"</p> }.into_any(),
            }}
        </div></section> },
    )
}

#[island(lazy)]
pub fn AdminHealthPage() -> impl IntoView {
    let health = browser_load(get_admin_health);
    page_shell(
        "Configuration health",
        "Verify the active storage, mail, and authorization profile.",
        view! { <section class="panel"><div class="client-data-slot">
            {move || match health.get() {
                Some(Ok(value)) => view! { <dl class="kv">
                    <dt>"Status"</dt><dd>{value.status}</dd>
                    <dt>"Storage"</dt><dd>{value.storage_backend}</dd>
                    <dt>"Mail"</dt><dd>{value.mail_transport}</dd>
                    <dt>"Authorization"</dt><dd>{value.authorization_provider}</dd>
                </dl> }.into_any(),
                Some(Err(error)) => view! { <p class="error-banner">{server_error_text(error)}</p> }.into_any(),
                None => view! { <p class="result-line">"Loading health"</p> }.into_any(),
            }}
        </div></section> },
    )
}

#[island(lazy)]
pub fn AdminPoliciesPage() -> impl IntoView {
    let versions = browser_load(list_policy_versions);
    let publish_action = ServerAction::<PublishPolicyVersion>::new();
    let (policy_text, set_policy_text) = signal(String::new());
    let (schema_text, set_schema_text) = signal(String::new());
    page_shell(
        "Cedar policy versions",
        "Validate and publish a versioned policy bundle with MFA step-up.",
        view! {
            <section class="panel"><h2>"Published versions"</h2><div class="client-data-slot">
                {move || match versions.get() {
                    Some(Ok(response)) => view! { <dl class="kv"><For each=move || response.versions.clone() key=|version| version.version_id.clone() children=move |version| view! {
                        <dt>{version.version_id}</dt><dd>{format!("{} / {}", version.status, version.policy_hash)}</dd>
                    } /></dl> }.into_any(),
                    Some(Err(error)) => view! { <p class="error-banner">{server_error_text(error)}</p> }.into_any(),
                    None => view! { <p class="result-line">"Loading versions"</p> }.into_any(),
                }}
            </div></section>
            <section class="panel"><h2>"Publish candidate"</h2>
                <label><span>"Cedar policy"</span><textarea prop:value=move || policy_text.get() on:input=move |event| set_policy_text.set(event_target_value(&event)) /></label>
                <label><span>"Cedar schema JSON"</span><textarea prop:value=move || schema_text.get() on:input=move |event| set_schema_text.set(event_target_value(&event)) /></label>
                <button type="button" class="primary-button" on:click=move |_| {
                    publish_action.dispatch(PublishPolicyVersion {
                        policy_text: policy_text.get_untracked(),
                        schema_text: schema_text.get_untracked(),
                    });
                }>"Validate and publish"</button>
                <Show when=move || publish_action.value().get().is_some()><p class="result-line">{move || action_result_text(publish_action.value().get())}</p></Show>
            </section>
        },
    )
}

#[component]
pub fn NotFoundPage() -> impl IntoView {
    set_page_status(http::StatusCode::NOT_FOUND);
    error_page_shell(
        "Not found",
        "This page does not exist.",
        view! { <ReturnToLoginLink /> },
    )
}

#[island(lazy)]
fn ProviderConfigForm() -> impl IntoView {
    let action = ServerAction::<SaveAuthProvider>::new();
    let pending = action.pending();
    let value = action.value();
    let (provider_id, set_provider_id) = signal("google".to_string());
    let (enabled, set_enabled) = signal(true);

    let submit = move |_| {
        action.dispatch(SaveAuthProvider {
            provider_id: provider_id.get_untracked(),
            enabled: enabled.get_untracked(),
        });
    };

    view! {
        <section class="panel">
            <h2>"Provider"</h2>
            <label>
                <span>"Provider id"</span>
                <input
                    type="text"
                    prop:value=move || provider_id.get()
                    on:input=move |event| set_provider_id.set(event_target_value(&event))
                />
            </label>
            <label class="inline-field">
                <input
                    type="checkbox"
                    prop:checked=move || enabled.get()
                    on:change=move |event| set_enabled.set(event_target_checked(&event))
                />
                <span>"Enabled"</span>
            </label>
            <button type="button" class="secondary-button" disabled=move || pending.get() on:click=submit>
                "Save provider"
            </button>
            <Show when=move || value.get().is_some()>
                <p class="result-line">{move || action_result_text(value.get())}</p>
            </Show>
        </section>
    }
}

#[island(lazy)]
fn SigningKeyRotationForm() -> impl IntoView {
    let rotate_action = ServerAction::<RotateSigningKey>::new();
    let pending = rotate_action.pending();
    let value = rotate_action.value();
    let (kid, set_kid) = signal("fullstack-app-next-hs256".to_string());
    let (retire_previous, set_retire_previous) = signal(true);
    let keys = browser_load(list_signing_keys);

    let submit = move |_| {
        rotate_action.dispatch(RotateSigningKey {
            kid: kid.get_untracked(),
            retire_previous: retire_previous.get_untracked(),
        });
    };

    view! {
        <section class="panel">
            <h2>"Signing key rotation"</h2>
            <p class="muted">"Requires a system-administrator session with MFA step-up."</p>
            <div class="client-data-slot">
                {move || match keys.get() {
                    Some(Ok(response)) if response.keys.is_empty() => view! {
                        <p class="result-line">"No signing keys are configured."</p>
                    }.into_any(),
                    Some(Ok(response)) => view! {
                        <dl class="kv">
                            <For
                                each=move || response.keys.clone()
                                key=|key| key.kid.clone()
                                children=move |key| view! {
                                    <dt>{key.kid}</dt>
                                    <dd>{format!("{} / {}{}", key.alg, key.status, if key.active { " / active" } else { "" })}</dd>
                                }
                            />
                        </dl>
                    }.into_any(),
                    Some(Err(error)) => view! {
                        <p class="result-line">{server_error_text(error)}</p>
                    }.into_any(),
                    None => view! { <p class="result-line">"Loading keys"</p> }.into_any(),
                }}
            </div>
            <label>
                <span>"Target key id"</span>
                <input
                    type="text"
                    prop:value=move || kid.get()
                    on:input=move |event| set_kid.set(event_target_value(&event))
                />
            </label>
            <label class="inline-field">
                <input
                    type="checkbox"
                    prop:checked=move || retire_previous.get()
                    on:change=move |event| set_retire_previous.set(event_target_checked(&event))
                />
                <span>"Retire previous active key"</span>
            </label>
            <button type="button" class="secondary-button" disabled=move || pending.get() on:click=submit>
                "Rotate key"
            </button>
            <Show when=move || value.get().is_some()>
                <p class="result-line">{move || action_result_text(value.get())}</p>
            </Show>
        </section>
    }
}

#[island(lazy)]
fn RedirectAllowlistForm() -> impl IntoView {
    let action = ServerAction::<SaveRedirectAllowlist>::new();
    let pending = action.pending();
    let value = action.value();
    let (redirects_json, set_redirects_json) = signal("[\"/account/profile\"]".to_string());

    let submit = move |_| {
        action.dispatch(SaveRedirectAllowlist {
            redirects_json: redirects_json.get_untracked(),
        });
    };

    view! {
        <section class="panel">
            <h2>"Allowed redirects"</h2>
            <textarea
                rows="5"
                prop:value=move || redirects_json.get()
                on:input=move |event| set_redirects_json.set(event_target_value(&event))
            />
            <button type="button" class="secondary-button" disabled=move || pending.get() on:click=submit>
                "Save allowlist"
            </button>
            <Show when=move || value.get().is_some()>
                <p class="result-line">{move || action_result_text(value.get())}</p>
            </Show>
        </section>
    }
}


pub(crate) fn browser_load<T, Fut, F>(load: F) -> ReadSignal<Option<T>>
where
    T: Send + Sync + 'static,
    Fut: std::future::Future<Output = T> + 'static,
    F: FnOnce() -> Fut + Send + Sync + 'static,
{
    let (value, set_value) = signal(None);

    #[cfg(feature = "hydrate")]
    {
        let load = StoredValue::new(Some(load));
        Effect::new(move |_| {
            if let Some(load) = load.write_value().take() {
                spawn_local(async move {
                    let _ = after_island_hydration().await;
                    set_value.set(Some(load().await));
                });
            }
        });
    }

    #[cfg(not(feature = "hydrate"))]
    let _ = (set_value, load);

    value
}


