#![allow(unused_imports)]
#![allow(clippy::unused_unit)] // Leptos `view! {}` expands to intentional unit views.
#![allow(clippy::unit_arg)] // Empty Leptos views intentionally pass unit to `into_any`.

use crate::contracts::{
    AcceptedResponse, AccountSessionListResponse, AccountSessionSummary, AdminUserListResponse,
    AuditEventListResponse, AuthCapabilities, AuthProviderSummary,
    AuthorizationCapabilitiesResponse, CapturedMailResponse, DashboardLayout,
    DashboardNotification, DashboardSnapshot, DataSourceUpsert, EmailPasswordLoginRequest,
    EmailPasswordRegisterRequest, EmailVerificationCompleteRequest, EmailVerificationResendRequest,
    HealthStatusResponse, InvitationAcceptRequest, InvitationCreateRequest, InvitationListResponse,
    LoginCompletionResponse, LogoutResponse, MembershipListResponse, MfaCodeRequest,
    MfaEnrollConfirmResponse, MfaEnrollStartResponse, MfaStatusResponse, OAuthCallbackRequest,
    OAuthStartResponse, OrganizationCreateRequest, OrganizationListResponse, OrganizationSummary,
    PasskeyStartRequest, PasskeyStartResponse, PasskeyVerifyRequest, PasswordChangeRequest,
    PasswordResetCompleteRequest, PasswordResetStartRequest, PasswordResetStartResponse,
    PolicyPublishRequest, PolicyVersionListResponse, ProfileUpdateRequest, ProfileView,
    PublicProfileView, RoleListResponse, RoleUpsertRequest, SecretCreateRequest,
    SessionRevokeRequest, SessionView, SigningKeyListResponse, SigningKeyRotateRequest,
    SigningKeyRotateResponse,
};
use crate::ui::{
    AuthBrand, ErrorBanner, Field, FieldGroup, Panel, PrimaryButton, SectionLabel, SuccessBanner,
    TextInput, account_page_shell, error_page_shell, page_shell, public_page_shell,
};
use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{
    components::*,
    hooks::{use_location, use_params_map},
    path,
};
use server_fn::codec::Json;

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

function isWorkspaceChromePath(pathname) {
  const path = (pathname || "/").replace(/\/+$/, "") || "/";
  if (path.startsWith("/onboarding")) {
    return false;
  }
  return (
    path === "/dashboard" ||
    path.startsWith("/dashboard/") ||
    path.startsWith("/account") ||
    path.startsWith("/organizations") ||
    path.startsWith("/org/") ||
    path.startsWith("/admin") ||
    path.startsWith("/invitations") ||
    path.startsWith("/auth/callback")
  );
}

function markWorkspaceNavActive() {
  window.dispatchEvent(new CustomEvent("workspace-nav-mark"));
}

/**
 * Option B: persistent workspace chrome.
 * Soft-nav swaps only content + primary nav + topbar title.
 * Sidebar foot islands (org switcher / account / theme) stay mounted.
 */
export function initWorkspaceChromePersist() {
  if (window.__workspaceChromePersistBound) {
    return;
  }
  window.__workspaceChromePersistBound = true;

  let navToken = 0;

  function hydrateNewIslands(root) {
    if (typeof window.__hydrateIsland !== "function") {
      return;
    }
    const scope = root || document;
    const islands = scope.querySelectorAll
      ? scope.querySelectorAll("leptos-island")
      : [];
    islands.forEach((island) => {
      if (island.$$hydrated) {
        return;
      }
      try {
        window.__hydrateIsland(island, island.dataset.component);
        island.$$hydrated = true;
      } catch (err) {
        console.error("chrome-persist hydrate failed", err);
      }
    });
  }

  function swapRegionById(doc, id) {
    const next = doc.getElementById(id);
    const current = document.getElementById(id);
    if (!next || !current || !current.parentNode) {
      return false;
    }
    const imported = document.importNode(next, true);
    current.replaceWith(imported);
    return true;
  }

  async function softNavigate(url, options) {
    const replace = options && options.replace;
    const token = ++navToken;
    const req = new Request(url, {
      headers: {
        Accept: "text/html",
        "Islands-Router": "true",
      },
      credentials: "same-origin",
    });

    let resp;
    try {
      resp = await fetch(req);
    } catch (err) {
      window.location.href = url;
      return;
    }

    if (token !== navToken) {
      return;
    }

    if (!resp.ok) {
      window.location.href = url;
      return;
    }

    const html = await resp.text();
    if (token !== navToken) {
      return;
    }

    const doc = new DOMParser().parseFromString(html, "text/html");
    if (!doc.getElementById("workspace-shell") || !document.getElementById("workspace-shell")) {
      // Left the workspace chrome tree (login, etc.) — full navigation.
      window.location.href = resp.redirected ? resp.url : url;
      return;
    }

    const apply = () => {
      // Never touch chrome-foot / theme / account / org switcher islands.
      swapRegionById(doc, "workspace-content");
      swapRegionById(doc, "workspace-primary-nav");
      swapRegionById(doc, "workspace-topbar-title");
      hydrateNewIslands(document.getElementById("workspace-content"));
      hydrateNewIslands(document.getElementById("workspace-primary-nav"));
      const titleEl = doc.querySelector("title");
      if (titleEl && titleEl.textContent) {
        document.title = titleEl.textContent;
      }
    };

    try {
      if (document.startViewTransition) {
        await document.startViewTransition(apply).finished.catch(() => {});
      } else {
        apply();
      }
    } catch (err) {
      console.error("chrome-persist soft nav failed", err);
      window.location.href = url;
      return;
    }

    const finalUrl = resp.redirected ? resp.url : url;
    if (replace) {
      window.history.replaceState(undefined, "", finalUrl);
    } else {
      window.history.pushState(undefined, "", finalUrl);
    }
    markWorkspaceNavActive();
  }

  document.addEventListener(
    "click",
    function (event) {
      if (
        event.defaultPrevented ||
        event.button !== 0 ||
        event.metaKey ||
        event.altKey ||
        event.ctrlKey ||
        event.shiftKey
      ) {
        return;
      }

      const anchor = event
        .composedPath()
        .find((el) => el instanceof Node && el.nodeName === "A");
      if (!anchor || !(anchor instanceof HTMLAnchorElement)) {
        return;
      }
      if (anchor.target || anchor.hasAttribute("download")) {
        return;
      }
      const rel = (anchor.getAttribute("rel") || "").split(/\s+/);
      if (rel.includes("external")) {
        return;
      }

      let url;
      try {
        url = new URL(anchor.href, document.baseURI);
      } catch (_) {
        return;
      }
      if (url.origin !== window.location.origin) {
        return;
      }
      // Same-document hash only.
      if (
        url.pathname === window.location.pathname &&
        url.search === window.location.search
      ) {
        return;
      }

      // Only intercept when both ends use workspace chrome.
      if (
        !isWorkspaceChromePath(window.location.pathname) ||
        !isWorkspaceChromePath(url.pathname)
      ) {
        return;
      }
      if (!document.getElementById("workspace-shell")) {
        return;
      }

      // Own the navigation so islands-router does not re-diff the whole tree
      // (which still remounts chrome when branch markers diverge).
      event.preventDefault();
      event.stopImmediatePropagation();
      softNavigate(url.href, { replace: false });
    },
    true
  );

  window.addEventListener(
    "popstate",
    function (event) {
      if (!isWorkspaceChromePath(window.location.pathname)) {
        return;
      }
      if (!document.getElementById("workspace-shell")) {
        return;
      }
      event.stopImmediatePropagation();
      softNavigate(window.location.href, { replace: true });
    },
    true
  );
}

export function bindWorkspaceNavActive() {
  function mark() {
    markWorkspaceNavActive();
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

// Click-away + Escape for sidebar/topbar <details> flyouts (account + workspace).
export function bindUserMenuDismiss() {
  if (window.__userMenuDismissBound) {
    return;
  }
  window.__userMenuDismissBound = true;

  // Keep in sync with WorkspaceUserMenu / WorkspaceOrgSwitcher markup
  // (`data-flyout` hooks — pure-utility class lists are not stable selectors).
  const FLYOUT_SELECTOR = '[data-flyout="user-menu"], [data-flyout="org-switcher"]';

  function closeOpenMenus(except) {
    document.querySelectorAll(FLYOUT_SELECTOR + "[open]").forEach((details) => {
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
      const openMenus = document.querySelectorAll(FLYOUT_SELECTOR + "[open]");
      if (!openMenus.length) {
        return;
      }
      openMenus.forEach((open) => {
        if (!open.contains(target)) {
          open.removeAttribute("open");
        }
      });
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

function passkeyLocalhostUrl() {
  const port = window.location.port ? ":" + window.location.port : "";
  return (
    "http://localhost" +
    port +
    window.location.pathname +
    window.location.search +
    window.location.hash
  );
}

function passkeyLoopbackHint() {
  const host = window.location.hostname;
  if (host === "127.0.0.1" || host === "::1" || host === "[::1]") {
    return (
      " Browsers reject IP addresses as WebAuthn rpId — open " +
      passkeyLocalhostUrl() +
      " (not 127.0.0.1). Session cookies are host-scoped; sign in again on localhost if needed."
    );
  }
  return "";
}

/** WebAuthn cannot use IP hosts. Prefer an automatic hop to localhost. */
function ensurePasskeyHostname() {
  const host = window.location.hostname;
  if (host === "127.0.0.1" || host === "::1" || host === "[::1]") {
    window.location.replace(passkeyLocalhostUrl());
    return false;
  }
  return true;
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
  // Auto-hop to localhost when the tab is still on 127.0.0.1 / ::1.
  if (!ensurePasskeyHostname()) {
    throw "Redirecting to localhost for WebAuthn (IP hosts are not valid rpIds).";
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
  if (!ensurePasskeyHostname()) {
    throw "Redirecting to localhost for WebAuthn (IP hosts are not valid rpIds).";
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

    #[wasm_bindgen(js_name = initWorkspaceChromePersist)]
    pub(crate) fn init_workspace_chrome_persist();

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
    pub(crate) async fn create_passkey_credential(options_json: String)
    -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, js_name = getPasskeyCredential)]
    pub(crate) async fn get_passkey_credential(
        options_json: String,
        mediation: String,
    ) -> Result<JsValue, JsValue>;
}

pub mod account;
pub mod admin;
pub mod auth;
pub mod dashboard;
pub mod helpers;
pub mod organizations;
pub mod path;
pub mod router;
pub mod server_fns;
pub mod access_ui;
pub mod theme;
pub mod workspace;
pub mod workspace_settings;

pub use account::*;
pub use admin::*;
pub use auth::*;
pub use dashboard::{DashboardHome, DashboardPage};
pub use helpers::*;
pub use organizations::*;
pub use path::*;
pub use router::App;
#[cfg(feature = "ssr")]
pub use router::shell;
pub use server_fns::*;
pub use access_ui::CanAccess;
pub use theme::ThemeToggle;
pub use workspace::{AppLayout, WorkspaceOnboardingGate, WorkspaceOnboardingPage, WorkspaceShell};
pub use workspace_settings::{
    LegacySettingsRedirect, WorkspaceSettingsAuditPage, WorkspaceSettingsDangerPage,
    WorkspaceSettingsGeneralPage, WorkspaceSettingsIndexRedirect, WorkspaceSettingsInvitationsPage,
    WorkspaceSettingsMembersPage, WorkspaceSettingsRolesPage, WorkspaceSettingsShell,
};

use crate::app::helpers::{
    action_result_text, current_browser_pathname, has_permission, next_url, optional_text,
    org_monogram, org_tone_index, redirect_browser, selected_action_error, selected_auth_error,
    server_error_text, short_id_label, validate_email_only, validate_login_form,
};
use crate::app::path::{is_workspace_path, workspace_topbar_title};
use crate::ui::classes::{
    AUTH_TEXT_LINK, BANNER_ERROR, BANNER_SUCCESS, BTN_AUTH_SUBMIT, BTN_PRIMARY, BTN_SECONDARY,
    BUTTON_ROW, CLIENT_DATA_SLOT, FIELD, FIELD_GROUP, HOME_ACTIONS, HOME_COPY, HOME_INTRO,
    HOME_KICKER, HOME_NOTE, HOME_STEP, HOME_STEP_INDEX, HOME_STEP_P, HOME_STEP_STRONG, HOME_STEPS,
    HOME_STEPS_LIST, HOME_TITLE, INPUT, PANEL, PANEL_COMPACT, RESULT_LINE, SECTION_LABEL,
};

#[component]
pub fn HomePage() -> impl IntoView {
    public_page_shell(
        "Production fullstack Rust",
        "Leptos islands, trusted authentication, embedded Cedar, DDD persistence, REST, and Spin gRPC in one component.",
        view! {
            <section class=HOME_INTRO>
                <p class=HOME_KICKER>"A calm starting point for a serious system"</p>
                <h2 class=HOME_TITLE>"One verified session for every surface."</h2>
                <p class=HOME_COPY>
                    "Create an account, verify your email, then move between the Leptos UI, REST endpoints, and authenticated gRPC services without changing the application boundary."
                </p>
                <div class=HOME_ACTIONS>
                    <a class=BTN_PRIMARY href="/register">"Create account"</a>
                    <a class=BTN_SECONDARY href="/login">"Sign in"</a>
                </div>
            </section>
            <section class=HOME_STEPS>
                <p class=SECTION_LABEL>"Start here"</p>
                <ol class=HOME_STEPS_LIST>
                    <li class=HOME_STEP>
                        <span class=HOME_STEP_INDEX>"01"</span>
                        <div>
                            <strong class=HOME_STEP_STRONG>"Register"</strong>
                            <p class=HOME_STEP_P>"Create a global account with a password or an enabled provider."</p>
                        </div>
                    </li>
                    <li class=HOME_STEP>
                        <span class=HOME_STEP_INDEX>"02"</span>
                        <div>
                            <strong class=HOME_STEP_STRONG>"Verify"</strong>
                            <p class=HOME_STEP_P>"Use the one-time link delivered by the configured mail transport."</p>
                        </div>
                    </li>
                    <li class=HOME_STEP>
                        <span class=HOME_STEP_INDEX>"03"</span>
                        <div>
                            <strong class=HOME_STEP_STRONG>"Organize"</strong>
                            <p class=HOME_STEP_P>"Create or join a tenant, then manage access from the protected workspace."</p>
                        </div>
                    </li>
                </ol>
                <p class=HOME_NOTE>"The browser shell stays server-rendered. Only interactive forms and protected controls hydrate."</p>
            </section>
        },
    )
}

/// Best-effort async sleep for hydrate (vault reveal auto-mask).
#[cfg(feature = "hydrate")]
async fn gloo_timers_sleep_ms(ms: u64) {
    use js_sys::Promise;
    use wasm_bindgen_futures::JsFuture;
    let promise = Promise::new(&mut |resolve, _reject| {
        if let Some(window) = web_sys::window() {
            let _ =
                window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms as i32);
        } else {
            let _ = resolve.call0(&wasm_bindgen::JsValue::NULL);
        }
    });
    let _ = JsFuture::from(promise).await;
}

#[cfg(not(feature = "hydrate"))]
async fn gloo_timers_sleep_ms(_ms: u64) {}

#[component]
pub fn NotFoundPage() -> impl IntoView {
    set_page_status(http::StatusCode::NOT_FOUND);
    error_page_shell(
        "Not found",
        "This page does not exist.",
        view! { <NotFoundRecoveryLink /> },
    )
}

/// Prefer Dashboard when a session exists; otherwise sign-in.
#[island]
pub fn NotFoundRecoveryLink() -> impl IntoView {
    let session = browser_load(get_current_session);
    view! {
        <div class=CLIENT_DATA_SLOT>
            {move || match session.get() {
                Some(Ok(view)) if view.authenticated => view! {
                    <a class=BTN_SECONDARY href="/dashboard">"Go to dashboard"</a>
                }
                .into_any(),
                _ => view! {
                    <a class=BTN_SECONDARY href="/login">"Return to sign in"</a>
                }
                .into_any(),
            }}
        </div>
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
