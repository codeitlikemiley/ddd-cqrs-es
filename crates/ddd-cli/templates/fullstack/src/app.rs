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
    async fn after_island_hydration() -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_name = initWorkspaceSidebar)]
    fn init_workspace_sidebar();

    #[wasm_bindgen(js_name = bindWorkspaceNavActive)]
    fn bind_workspace_nav_active();

    #[wasm_bindgen(js_name = bindUserMenuDismiss)]
    fn bind_user_menu_dismiss();

    #[wasm_bindgen(catch, js_name = pickImageDataUrl)]
    async fn pick_image_data_url(
        input: web_sys::HtmlInputElement,
        max_bytes: u32,
    ) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_name = passkeySupported)]
    fn passkey_supported() -> bool;

    #[wasm_bindgen(catch, js_name = isConditionalMediationAvailable)]
    async fn is_conditional_mediation_available() -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, js_name = copyText)]
    async fn copy_text(value: String) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, js_name = createPasskeyCredential)]
    async fn create_passkey_credential(options_json: String) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, js_name = getPasskeyCredential)]
    async fn get_passkey_credential(
        options_json: String,
        mediation: String,
    ) -> Result<JsValue, JsValue>;
}

#[cfg(feature = "ssr")]
pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                // Apply sidebar preference before first paint so every navigation
                // keeps mini/hidden without flashing full → mini.
                <script>
                    {r#"(function(){try{var m=localStorage.getItem("workspace-sidebar-mode");if(m==="mini"||m==="hidden"||m==="full"){document.documentElement.setAttribute("data-sidebar-pref",m);}}catch(e){}})();"#}
                </script>
                <AutoReload options=options.clone() />
                <HydrationScripts options=options.clone() islands=true root="" />
                <MetaTags />
            </head>
            <body>
                <App />
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    let fallback = || view! { <NotFoundPage /> }.into_view();

    // ParentRoute + Outlet: workspace chrome mounts once and is reused across
    // authenticated navigations (islands-router). Only page content swaps.
    view! {
        <Stylesheet id="leptos" href="/pkg/fullstack_app.css" />
        // Inline SVG avoids a static-file route for /favicon.ico.
        <Link
            rel="icon"
            type_="image/svg+xml"
            href="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'%3E%3Crect width='32' height='32' rx='8' fill='%230d0d0d'/%3E%3Ctext x='16' y='22' text-anchor='middle' font-family='system-ui,sans-serif' font-size='16' font-weight='700' fill='%23fff'%3Ed%3C/text%3E%3C/svg%3E"
        />
        <Meta name="description" content="Production fullstack Rust with Leptos islands, verified sessions, REST, and Spin gRPC." />
        <Title text="wasi-auth / fullstack" />

        <Router>
            <Routes fallback>
                <ParentRoute path=path!("") view=AppLayout>
                    <Route path=path!("") view=HomePage />
                    <Route path=path!("/login") view=LoginPage />
                    <Route path=path!("/register") view=RegisterPage />
                    <Route path=path!("/forgot-password") view=ForgotPasswordPage />
                    <Route path=path!("/reset-password") view=ResetPasswordPage />
                    <Route path=path!("/verify-email") view=VerifyEmailPage />
                    <Route path=path!("/verify-email/pending") view=VerificationPendingPage />
                    <Route path=path!("/verify-email/resend") view=ResendVerificationPage />
                    <Route path=path!("/dashboard") view=DashboardPage />
                    <Route path=path!("/invitations/accept") view=InvitationAcceptPage />
                    <Route path=path!("/auth/callback/:provider") view=OAuthCallbackPage />
                    <Route path=path!("/auth/callback/:provider/error") view=OAuthCallbackErrorPage />
                    <Route path=path!("/auth/required") view=AuthRequiredPage />
                    <Route path=path!("/auth/forbidden") view=ForbiddenPage />
                    <Route path=path!("/auth/session-expired") view=SessionExpiredPage />
                    <Route path=path!("/auth/passkey-unsupported") view=PasskeyUnsupportedPage />
                    <Route path=path!("/account/profile") view=AccountProfilePage />
                    <Route path=path!("/account/password") view=AccountPasswordPage />
                    <Route path=path!("/account/providers") view=AccountProvidersPage />
                    <Route path=path!("/account/passkeys") view=AccountPasskeysPage />
                    <Route path=path!("/account/mfa") view=AccountMfaPage />
                    <Route path=path!("/account/sessions") view=AccountSessionsPage />
                    <Route path=path!("/account/vault") view=AccountVaultRedirectPage />
                    <Route path=path!("/onboarding/workspace") view=WorkspaceOnboardingPage />
                    <Route path=path!("/org/:slug/vault") view=OrgVaultPage />
                    <Route path=path!("/u/:handle") view=PublicProfilePage />
                    <Route path=path!("/organizations") view=OrganizationsPage />
                    <Route path=path!("/organizations/settings") view=OrganizationSettingsPage />
                    <Route path=path!("/organizations/members") view=OrganizationMembersPage />
                    <Route path=path!("/organizations/invitations") view=OrganizationInvitationsPage />
                    <Route path=path!("/organizations/roles") view=OrganizationRolesPage />
                    <Route path=path!("/organizations/permissions") view=OrganizationPermissionsPage />
                    <Route path=path!("/organizations/audit") view=OrganizationAuditPage />
                    <Route path=path!("/admin/users") view=AdminUsersPage />
                    <Route path=path!("/admin/health") view=AdminHealthPage />
                    <Route path=path!("/admin/policies") view=AdminPoliciesPage />
                    <Route path=path!("/admin/auth/signing-keys") view=SigningKeyAdminPage />
                    <Route path=path!("/admin/auth/providers") view=AuthProviderAdminPage />
                    <Route path=path!("/admin/auth/redirects") view=RedirectAllowlistPage />
                    <Route path=path!("/admin/authorization/policy") view=AuthorizationPolicyPage />
                    <Route path=path!("/*any") view=NotFoundPage />
                </ParentRoute>
            </Routes>
        </Router>
    }
}

fn is_workspace_path(path: &str) -> bool {
    let path = path.trim_end_matches('/');
    path == "/dashboard"
        || path.starts_with("/dashboard/")
        || path.starts_with("/account")
        || path.starts_with("/organizations")
        || path.starts_with("/org/")
        || path.starts_with("/onboarding")
        || path.starts_with("/admin")
        || path.starts_with("/invitations")
        || path.starts_with("/auth/callback")
}

fn workspace_topbar_title(path: &str) -> &'static str {
    let path = path.trim_end_matches('/');
    if path == "/dashboard" || path.is_empty() {
        "Dashboard"
    } else if path.starts_with("/account/profile") {
        "Profile"
    } else if path.starts_with("/account/password") {
        "Password"
    } else if path.starts_with("/account/providers") {
        "Providers"
    } else if path.starts_with("/account/passkeys") {
        "Passkeys"
    } else if path.starts_with("/account/mfa") {
        "MFA"
    } else if path.starts_with("/account/sessions") {
        "Sessions"
    } else if path.starts_with("/account/vault") || path.contains("/vault") {
        "Secret vault"
    } else if path.starts_with("/onboarding") {
        "Create workspace"
    } else if path.starts_with("/account") {
        "Account"
    } else if path.starts_with("/organizations/settings") {
        "Organization settings"
    } else if path.starts_with("/organizations/members") {
        "Members"
    } else if path.starts_with("/organizations/invitations") {
        "Invitations"
    } else if path.starts_with("/organizations/roles") {
        "Roles"
    } else if path.starts_with("/organizations/permissions") {
        "Permissions"
    } else if path.starts_with("/organizations/audit") {
        "Audit"
    } else if path.starts_with("/organizations") {
        "Organizations"
    } else if path.starts_with("/admin/users") {
        "Users"
    } else if path.starts_with("/admin/health") {
        "Health"
    } else if path.starts_with("/admin/policies") {
        "Policies"
    } else if path.starts_with("/admin/auth/signing-keys") {
        "Signing keys"
    } else if path.starts_with("/admin/auth/providers") {
        "Auth providers"
    } else if path.starts_with("/admin/auth/redirects") {
        "Redirects"
    } else if path.starts_with("/admin/authorization") {
        "Authorization"
    } else if path.starts_with("/admin") {
        "Admin"
    } else if path.starts_with("/invitations") {
        "Invitation"
    } else {
        "Workspace"
    }
}

/// Root layout: keep the workspace shell mounted across workspace navigations.
#[component]
fn AppLayout() -> impl IntoView {
    let location = use_location();
    // Memo only flips when entering/leaving the workspace chrome — not on every path.
    let workspace_mode =
        Memo::new(move |_| is_workspace_path(&location.pathname.get()));

    view! {
        {move || {
            if workspace_mode.get() {
                view! {
                    <WorkspaceShell>
                        <Outlet />
                    </WorkspaceShell>
                }
                .into_any()
            } else {
                view! {
                    <main class="auth-shell">
                        <Outlet />
                    </main>
                }
                .into_any()
            }
        }}
    }
}

/// Persistent workspace chrome. Children render in the main content outlet.
#[component]
fn WorkspaceShell(children: Children) -> impl IntoView {
    let location = use_location();
    let topbar_title =
        Memo::new(move |_| workspace_topbar_title(&location.pathname.get()).to_string());

    view! {
        <WorkspaceOnboardingGate />
        <div class="workspace-shell" id="workspace-shell" data-sidebar="full">
            <script>
                {r#"(function(){try{var s=document.getElementById("workspace-shell");if(!s)return;var m=localStorage.getItem("workspace-sidebar-mode");if(m==="mini"||m==="hidden"||m==="full"){s.setAttribute("data-sidebar",m);}}catch(e){}})();"#}
            </script>
            <input
                type="checkbox"
                id="workspace-nav-toggle"
                class="workspace-nav-toggle"
                aria-controls="workspace-sidebar"
            />
            <label
                class="workspace-nav-backdrop"
                for="workspace-nav-toggle"
                aria-label="Close navigation"
            ></label>
            <aside class="workspace-sidebar" id="workspace-sidebar" aria-label="Workspace">
                <div class="workspace-sidebar-top">
                    <a class="workspace-brand" href="/dashboard" aria-label="Workspace home">
                        <span class="workspace-brand-mark" aria-hidden="true">"d"</span>
                        <span class="workspace-brand-copy">
                            <strong>"wasi-auth"</strong>
                            <small>"workspace"</small>
                        </span>
                    </a>
                    <button
                        type="button"
                        class="workspace-sidebar-rail-toggle"
                        data-sidebar-action="toggle-mini"
                        aria-label="Toggle mini sidebar"
                        title="Toggle mini sidebar"
                    >
                        <span class="workspace-sidebar-rail-icon" aria-hidden="true"></span>
                    </button>
                    <label
                        class="workspace-sidebar-close"
                        for="workspace-nav-toggle"
                        aria-label="Close navigation"
                    >
                        "Close"
                    </label>
                </div>
                <nav class="workspace-nav" aria-label="Authenticated workspace">
                    <a class="workspace-nav-link" href="/dashboard" data-nav="overview" title="Overview">
                        <span class="workspace-nav-icon" aria-hidden="true" data-icon="overview"></span>
                        <span class="workspace-nav-text">"Overview"</span>
                    </a>
                    <a class="workspace-nav-link" href="/organizations" data-nav="organizations" title="Organizations">
                        <span class="workspace-nav-icon" aria-hidden="true" data-icon="organizations"></span>
                        <span class="workspace-nav-text">"Organizations"</span>
                    </a>
                    <WorkspaceSystemNav />
                </nav>
                <div class="workspace-sidebar-foot">
                    <WorkspaceOrgSwitcher />
                    <WorkspaceUserMenu />
                </div>
            </aside>
            <div class="workspace-main">
                <header class="workspace-topbar">
                    <label
                        class="workspace-menu-button workspace-menu-button-mobile"
                        for="workspace-nav-toggle"
                        aria-label="Open navigation"
                        aria-controls="workspace-sidebar"
                    >
                        <span class="workspace-menu-button-bars" aria-hidden="true">
                            <span></span>
                            <span></span>
                            <span></span>
                        </span>
                    </label>
                    <a class="workspace-topbar-brand" href="/dashboard" aria-label="Workspace home">
                        <span class="workspace-brand-mark" aria-hidden="true">"d"</span>
                        <span class="workspace-brand-copy">
                            <strong>"wasi-auth"</strong>
                            <small>"workspace"</small>
                        </span>
                    </a>
                    <div class="workspace-topbar-title">
                        <span class="workspace-topbar-page">{move || topbar_title.get()}</span>
                    </div>
                    <div class="workspace-topbar-org">
                        <WorkspaceOrgSwitcher />
                    </div>
                    <button
                        type="button"
                        class="workspace-menu-button workspace-menu-button-desktop"
                        data-sidebar-action="toggle-visibility"
                        aria-label="Show sidebar"
                        title="Show sidebar (⌘B)"
                    >
                        <span class="workspace-menu-button-bars" aria-hidden="true">
                            <span></span>
                            <span></span>
                            <span></span>
                        </span>
                    </button>
                    <WorkspaceSidebarControls />
                    <WorkspaceNavActive />
                </header>
                <div class="workspace-content">
                    {children()}
                </div>
            </div>
        </div>
    }
}

#[component]
fn HomePage() -> impl IntoView {
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
fn LoginPage() -> impl IntoView {
    view! {
        <div class="auth-page">
            <ExistingSessionRedirect />
            <section class="auth-card">
                <AuthBrand />
                <EmailPasswordAuthForm register_default=false />
            </section>
        </div>
    }
}

#[component]
fn RegisterPage() -> impl IntoView {
    view! {
        <div class="auth-page">
            <ExistingSessionRedirect />
            <section class="auth-card">
                <AuthBrand />
                <EmailPasswordAuthForm register_default=true />
            </section>
        </div>
    }
}

#[component]
fn ForgotPasswordPage() -> impl IntoView {
    view! {
        <div class="auth-page">
            <ExistingSessionRedirect />
            <section class="auth-card">
                <AuthBrand />
                <ForgotPasswordForm />
            </section>
        </div>
    }
}

#[component]
fn ResetPasswordPage() -> impl IntoView {
    // Do not mount ExistingSessionRedirect here. Tokenized reset links must
    // render the form even when a stale session cookie is still present.
    view! {
        <div class="auth-page">
            <section class="auth-card">
                <AuthBrand />
                <ResetPasswordForm />
            </section>
        </div>
    }
}

#[component]
fn InvitationAcceptPage() -> impl IntoView {
    // Authenticated document shell; unauthenticated browsers are redirected by
    // protected_ui_redirect with next= preserving ?token=.
    view! {
        <div class="auth-page">
            <section class="auth-card">
                <AuthBrand />
                <InvitationAcceptForm />
            </section>
        </div>
    }
}

#[component]
fn VerifyEmailPage() -> impl IntoView {
    view! {
        <div class="auth-page">
            <section class="auth-card">
                <AuthBrand />
                <EmailVerificationForm />
            </section>
        </div>
    }
}

#[component]
fn VerificationPendingPage() -> impl IntoView {
    view! {
        <div class="auth-page">
            <section class="auth-card">
                <AuthBrand />
                <section class="auth-form">
                    <div>
                        <p class="auth-kicker">"Email verification"</p>
                        <h1 class="auth-title">"Check your inbox"</h1>
                        <p class="auth-copy">
                            "Your account is pending. Open the one-time verification link before signing in."
                        </p>
                    </div>
                    <p class="auth-notice">
                        "Local capture mode keeps messages on this machine. Start the app with `make dev` to run delivery automatically."
                    </p>
                    <a class="auth-text-link" href="/verify-email/resend">"Send another verification link"</a>
                </section>
            </section>
        </div>
    }
}

#[component]
fn ResendVerificationPage() -> impl IntoView {
    view! {
        <div class="auth-page">
            <section class="auth-card">
                <AuthBrand />
                <ResendVerificationForm />
            </section>
        </div>
    }
}

#[component]
fn AuthBrand() -> impl IntoView {
    view! {
        <div class="auth-brand">
            <span class="auth-logo" aria-hidden="true">"d"</span>
            <div>
                <p class="auth-brand-name">"wasi-auth"</p>
                <p class="auth-brand-meta">"Secure workspace access"</p>
            </div>
        </div>
    }
}

#[island]
fn ExistingSessionRedirect() -> impl IntoView {
    let session = browser_load(get_current_session);

    view! {
        <div class="client-data-slot">
            {move || {
                if let Some(Ok(session)) = session.get()
                    && session.authenticated
                {
                    redirect_browser(&next_url());
                }
                view! {}
            }}
        </div>
    }
}

#[path = "app_dashboard_board.rs"]
mod dashboard_board;
pub use dashboard_board::{DashboardHome, DashboardPage};

fn short_id_label(id: &str) -> String {
    let trimmed = id.trim();
    if trimmed.len() <= 12 {
        return trimmed.to_owned();
    }
    let head: String = trimmed.chars().take(6).collect();
    let tail: String = trimmed
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("{head}…{tail}")
}

#[component]
fn OAuthCallbackPage() -> impl IntoView {
    page_shell(
        "Completing sign-in",
        "The provider callback will be verified by the server.",
        view! { <OAuthCallbackStatus /> },
    )
}

#[component]
fn OAuthCallbackErrorPage() -> impl IntoView {
    set_page_status(http::StatusCode::BAD_REQUEST);
    error_page_shell(
        "Sign-in failed",
        "The provider response could not be accepted.",
        view! { <ReturnToLoginLink /> },
    )
}

#[component]
fn AuthRequiredPage() -> impl IntoView {
    set_page_status(http::StatusCode::UNAUTHORIZED);
    error_page_shell(
        "Authentication required",
        "Sign in before continuing.",
        view! { <LoginRedirectLink /> },
    )
}

#[component]
fn ForbiddenPage() -> impl IntoView {
    set_page_status(http::StatusCode::FORBIDDEN);
    error_page_shell(
        "Access denied",
        "The current account cannot open this page.",
        view! {
            <div class="actions">
                <a class="link-button" href="/account/sessions">"Sessions"</a>
                <LogoutForm />
            </div>
        },
    )
}

#[component]
fn SessionExpiredPage() -> impl IntoView {
    set_page_status(http::StatusCode::UNAUTHORIZED);
    error_page_shell(
        "Session expired",
        "Sign in again to continue.",
        view! { <LoginRedirectLink /> },
    )
}

#[island(lazy)]
fn PasskeyUnsupportedPage() -> impl IntoView {
    error_page_shell(
        "Passkey unavailable",
        "Use email and password or an enabled provider.",
        view! { <OAuthProviderList /> },
    )
}

#[component]
fn AuthProviderAdminPage() -> impl IntoView {
    page_shell(
        "Auth providers",
        "Configure OAuth and OIDC providers.",
        view! { <ProviderConfigForm /> },
    )
}

#[component]
fn SigningKeyAdminPage() -> impl IntoView {
    page_shell(
        "Signing keys",
        "Rotate the active access-token signing key.",
        view! { <SigningKeyRotationForm /> },
    )
}

#[component]
fn RedirectAllowlistPage() -> impl IntoView {
    page_shell(
        "Redirect allowlist",
        "Restrict browser redirect targets.",
        view! { <RedirectAllowlistForm /> },
    )
}

#[island(lazy)]
fn AuthorizationPolicyPage() -> impl IntoView {
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

#[component]
fn AccountProfilePage() -> impl IntoView {
    account_page_shell(
        "Profile",
        "Your name, @handle, avatar, and whether others can find you.",
        "profile",
        view! { <AccountProfileCard /> },
    )
}

#[component]
fn PublicProfilePage() -> impl IntoView {
    let params = use_params_map();
    public_page_shell(
        "Profile",
        "Public account",
        view! {
            {move || {
                let handle = params
                    .get()
                    .get("handle")
                    .map(|value| value.to_string())
                    .unwrap_or_default();
                view! { <PublicProfileCard handle=handle /> }.into_any()
            }}
        },
    )
}

#[component]
fn AccountPasswordPage() -> impl IntoView {
    account_page_shell(
        "Password",
        "Update the password for this account. Enter your current password to confirm.",
        "password",
        view! { <ChangePasswordForm /> },
    )
}

#[island(lazy)]
fn AccountProvidersPage() -> impl IntoView {
    account_page_shell(
        "Providers",
        "Social sign-in options for this deployment. Enabled providers can be used on the login page.",
        "providers",
        view! { <AccountProvidersPanel /> },
    )
}

/// Known OAuth brands for the account Providers tab (always shown; greyed when off).
#[derive(Clone, Copy)]
struct ProviderBrand {
    id: &'static str,
    name: &'static str,
}

const PROVIDER_CATALOG: &[ProviderBrand] = &[
    ProviderBrand {
        id: "google",
        name: "Google",
    },
    ProviderBrand {
        id: "facebook",
        name: "Facebook",
    },
    ProviderBrand {
        id: "apple",
        name: "Apple",
    },
];

#[island(lazy)]
fn AccountProvidersPanel() -> impl IntoView {
    let providers = browser_load(list_auth_providers);

    view! {
        <section class="panel providers-panel">
            <div class="session-panel-head">
                <div>
                    <p class="section-label">"Social login"</p>
                    <h2>"Identity providers"</h2>
                </div>
            </div>
            <p class="passkey-lede">
                "These providers appear on the sign-in page when credentials are configured and OAuth is enabled. Greyed tiles are available but not active on this deployment."
            </p>

            <div class="provider-catalog">
                {PROVIDER_CATALOG
                    .iter()
                    .copied()
                    .map(|brand| {
                        view! {
                            <ProviderCatalogCard brand=brand providers=providers />
                        }
                    })
                    .collect_view()}
            </div>

            <p class="providers-empty-note">
                {move || match providers.get() {
                    None => "Loading provider status…".to_string(),
                    Some(Ok(list)) if list.is_empty() => {
                        "No providers are enabled.".to_string()
                    }
                    Some(Ok(list)) => {
                        let n = list.iter().filter(|p| p.enabled).count();
                        if n == 0 {
                            "No providers are enabled.".to_string()
                        } else {
                            format!(
                                "{n} provider{} enabled for sign-in.",
                                if n == 1 { "" } else { "s" }
                            )
                        }
                    }
                    Some(Err(error)) => server_error_text(error),
                }}
            </p>
        </section>
    }
}

#[component]
fn ProviderCatalogCard(
    brand: ProviderBrand,
    providers: ReadSignal<Option<Result<Vec<AuthProviderSummary>, ServerFnError>>>,
) -> impl IntoView {
    let brand_id = brand.id;
    let brand_name = brand.name;
    let is_enabled = move || {
        providers.get().and_then(Result::ok).is_some_and(|list| {
            list.iter()
                .any(|p| p.provider_id.eq_ignore_ascii_case(brand_id) && p.enabled)
        })
    };

    view! {
        <div
            class="provider-card"
            class:is-enabled=move || is_enabled()
            class:is-disabled=move || !is_enabled()
            data-provider=brand_id
        >
            <span class="provider-logo" aria-hidden="true" inner_html=provider_logo_svg(brand_id)></span>
            <span class="provider-card-body">
                <span class="provider-name">{brand_name}</span>
                <span class="provider-status">
                    {move || if is_enabled() { "Enabled" } else { "Not configured" }}
                </span>
            </span>
        </div>
    }
}

fn provider_logo_svg(provider_id: &str) -> String {
    // Simple monochrome brand marks; CSS greys them when disabled.
    match provider_id {
        "google" => r#"<svg viewBox="0 0 24 24" width="28" height="28" xmlns="http://www.w3.org/2000/svg" fill="currentColor" aria-hidden="true"><path d="M21.35 11.1h-9.18v2.96h5.27c-.23 1.5-1.72 4.4-5.27 4.4-3.17 0-5.76-2.62-5.76-5.86s2.59-5.86 5.76-5.86c1.8 0 3.01.77 3.7 1.43l2.52-2.43C16.99 4.33 15.03 3.4 12.17 3.4 6.99 3.4 2.8 7.58 2.8 12.6s4.19 9.2 9.37 9.2c5.41 0 8.99-3.8 8.99-9.15 0-.61-.07-1.08-.16-1.55z"/></svg>"#.to_owned(),
        "facebook" => r#"<svg viewBox="0 0 24 24" width="28" height="28" xmlns="http://www.w3.org/2000/svg" fill="currentColor" aria-hidden="true"><path d="M13.5 22v-8.1h2.72l.41-3.17h-3.13V8.7c0-.92.25-1.54 1.57-1.54H16.8V4.32C16.4 4.27 15.2 4.16 13.8 4.16c-2.9 0-4.88 1.77-4.88 5.02v2.8H6.2v3.17h2.72V22h4.58z"/></svg>"#.to_owned(),
        "apple" => r#"<svg viewBox="0 0 24 24" width="28" height="28" xmlns="http://www.w3.org/2000/svg" fill="currentColor" aria-hidden="true"><path d="M16.37 12.64c.02 2.3 2.02 3.07 2.04 3.08-.02.06-.32 1.1-1.05 2.18-.63.93-1.29 1.86-2.32 1.88-1.01.02-1.34-.6-2.5-.6-1.16 0-1.52.58-2.48.62-1 .04-1.76-.98-2.4-1.91-1.31-1.9-2.31-5.37-1-7.72.68-1.21 1.9-1.98 3.22-2 1-.02 1.95.68 2.5.68.55 0 1.8-.84 3.03-.71.52.02 1.97.21 2.9 1.58-.08.05-1.73 1.01-1.72 3.02zM14.9 6.5c.54-.66.91-1.57.81-2.48-.78.03-1.73.52-2.29 1.18-.5.58-.94 1.51-.82 2.4.87.07 1.76-.44 2.3-1.1z"/></svg>"#.to_owned(),
        _ => r#"<svg viewBox="0 0 24 24" width="28" height="28" xmlns="http://www.w3.org/2000/svg" fill="currentColor" aria-hidden="true"><circle cx="12" cy="12" r="9"/></svg>"#.to_owned(),
    }
}

#[component]
fn AccountPasskeysPage() -> impl IntoView {
    account_page_shell(
        "Passkeys",
        "Sign in with Face ID, Touch ID, Windows Hello, or a security key — no password to type.",
        "passkeys",
        view! { <PasskeyManager /> },
    )
}

#[component]
fn AccountMfaPage() -> impl IntoView {
    account_page_shell(
        "Authenticator app",
        "Protect sign-in with a time-based code from an app you already trust.",
        "mfa",
        view! { <MfaManager /> },
    )
}

/// Standard TOTP enrollment UX (GitHub / Google / Auth0 pattern).
/// Exclusive phases (only one surface mounted at a time):
/// overview → preparing → scan/confirm → recovery codes → enrolled tools.
#[island(lazy)]
fn MfaManager() -> impl IntoView {
    let status = browser_load(get_mfa_status);
    let start = ServerAction::<StartTotpEnrollment>::new();
    let confirm = ServerAction::<ConfirmTotpEnrollment>::new();
    let verify = ServerAction::<VerifyTotpStepUp>::new();
    let recover = ServerAction::<VerifyRecoveryCode>::new();
    let (enroll_code, set_enroll_code) = signal(String::new());
    let (step_up_code, set_step_up_code) = signal(String::new());
    let (recovery_code, set_recovery_code) = signal(String::new());
    let (show_manual_secret, set_show_manual_secret) = signal(false);
    let (copy_feedback, set_copy_feedback) = signal(String::new());
    let (recovery_saved, set_recovery_saved) = signal(false);

    view! {
        <div class="mfa-flow">
            {move || {
                // Recovery codes after confirm — exclusive focus
                if let Some(Ok(value)) = confirm.value().get() {
                    let codes = value.recovery_codes.clone();
                    let codes_for_copy = codes.join("\n");
                    return view! {
                        <div class="mfa-flow-focus-wrap">
                            <section class="panel mfa-recovery-panel">
                                <div class="mfa-wizard-progress" aria-hidden="true">
                                    <span class="mfa-wizard-step is-done">"1"</span>
                                    <span class="mfa-wizard-line is-done"></span>
                                    <span class="mfa-wizard-step is-done">"2"</span>
                                    <span class="mfa-wizard-line is-done"></span>
                                    <span class="mfa-wizard-step is-done">"3"</span>
                                </div>
                                <span class="mfa-badge mfa-badge-on">"Authenticator enabled"</span>
                                <h2>"Save your recovery codes"</h2>
                                <p class="mfa-lede mfa-lede-warn">
                                    "These codes are the only way back in if you lose your phone. Each code works once. We will not show them again."
                                </p>
                                <ul class="mfa-recovery-grid">
                                    <For
                                        each=move || codes.clone()
                                        key=|code| code.clone()
                                        children=move |code| view! { <li><code>{code}</code></li> }
                                    />
                                </ul>
                                <div class="button-row">
                                    <button
                                        type="button"
                                        class="secondary-button"
                                        on:click=move |_| {
                                            let value = codes_for_copy.clone();
                                            #[cfg(feature = "hydrate")]
                                            {
                                                spawn_local(async move {
                                                    let _ = copy_text(value).await;
                                                    set_copy_feedback.set("Recovery codes copied".to_owned());
                                                });
                                            }
                                            #[cfg(not(feature = "hydrate"))]
                                            {
                                                let _ = value;
                                            }
                                        }
                                    >"Copy all codes"</button>
                                </div>
                                <label class="mfa-ack">
                                    <input
                                        type="checkbox"
                                        prop:checked=move || recovery_saved.get()
                                        on:change=move |event| {
                                            set_recovery_saved.set(event_target_checked(&event));
                                        }
                                    />
                                    <span>"I stored these recovery codes in a password manager or offline safe place."</span>
                                </label>
                                <p class="mfa-hint mfa-copy-feedback" hidden=move || copy_feedback.get().is_empty()>
                                    {move || copy_feedback.get()}
                                </p>
                                <a
                                    class="primary-button"
                                    href="/account/mfa"
                                    class:is-disabled=move || !recovery_saved.get()
                                    on:click=move |ev| {
                                        if !recovery_saved.get_untracked() {
                                            ev.prevent_default();
                                            set_copy_feedback.set("Check the box below after you store the codes.".to_owned());
                                        }
                                    }
                                >"Finish setup"</a>
                            </section>
                        </div>
                    }.into_any();
                }

                // Preparing QR — exclusive
                if start.pending().get() {
                    return view! {
                        <div class="mfa-flow-focus-wrap">
                            <section class="panel mfa-enroll-panel mfa-enroll-loading">
                                <div class="mfa-wizard-progress" aria-hidden="true">
                                    <span class="mfa-wizard-step is-active">"1"</span>
                                    <span class="mfa-wizard-line"></span>
                                    <span class="mfa-wizard-step">"2"</span>
                                    <span class="mfa-wizard-line"></span>
                                    <span class="mfa-wizard-step">"3"</span>
                                </div>
                                <p class="section-label">"Step 1 of 3"</p>
                                <h2>"Preparing your authenticator setup"</h2>
                                <p class="mfa-lede">"Generating a one-time secret and QR code. Keep this tab open."</p>
                                <p class="result-line">"Preparing QR code…"</p>
                            </section>
                        </div>
                    }.into_any();
                }

                // Scan + confirm — exclusive (status/intro unmounted)
                if let Some(Ok(enrollment)) = start.value().get() {
                    let secret = enrollment.secret_base32.clone();
                    let uri = enrollment.provisioning_uri.clone();
                    let qr_svg = otpauth_qr_svg(&uri);
                    let secret_for_copy = secret.clone();
                    return view! {
                        <div class="mfa-flow-focus-wrap">
                            <section class="panel mfa-enroll-panel mfa-enroll-focus">
                                <div class="mfa-wizard-progress" aria-hidden="true">
                                    <span class="mfa-wizard-step is-done">"1"</span>
                                    <span class="mfa-wizard-line is-done"></span>
                                    <span class="mfa-wizard-step is-active">"2"</span>
                                    <span class="mfa-wizard-line"></span>
                                    <span class="mfa-wizard-step">"3"</span>
                                </div>
                                <p class="section-label">"Step 2 of 3 · Setup only"</p>
                                <h2>"Scan this QR code"</h2>
                                <p class="mfa-lede">
                                    "Open your authenticator app, choose add account, then point the camera at this code."
                                </p>
                                <div class="mfa-enroll-grid">
                                    <div class="mfa-qr-card">
                                        <div class="mfa-qr" inner_html=qr_svg></div>
                                        <p class="mfa-qr-caption">"Works with Google Authenticator, 1Password, Authy, Microsoft Authenticator, and others."</p>
                                    </div>
                                    <div class="mfa-enroll-side">
                                        <div class="mfa-manual">
                                            <button
                                                type="button"
                                                class="text-link mfa-manual-toggle"
                                                on:click=move |_| set_show_manual_secret.update(|open| *open = !*open)
                                            >
                                                {move || if show_manual_secret.get() {
                                                    "Hide manual entry key"
                                                } else {
                                                    "Can't scan? Enter key manually"
                                                }}
                                            </button>
                                            <div class="mfa-manual-body" hidden=move || !show_manual_secret.get()>
                                                <p class="mfa-hint">"Type this secret into your app. Spaces are optional."</p>
                                                <div class="mfa-secret-row">
                                                    <code class="mfa-secret">{secret.clone()}</code>
                                                    <button
                                                        type="button"
                                                        class="secondary-button"
                                                        on:click=move |_| {
                                                            let value = secret_for_copy.clone();
                                                            #[cfg(feature = "hydrate")]
                                                            {
                                                                spawn_local(async move {
                                                                    match copy_text(value).await {
                                                                        Ok(_) => set_copy_feedback.set("Secret copied".to_owned()),
                                                                        Err(_) => set_copy_feedback.set("Copy failed — select the secret manually".to_owned()),
                                                                    }
                                                                });
                                                            }
                                                            #[cfg(not(feature = "hydrate"))]
                                                            {
                                                                let _ = value;
                                                            }
                                                        }
                                                    >"Copy"</button>
                                                </div>
                                                <p class="mfa-hint mfa-copy-feedback" hidden=move || copy_feedback.get().is_empty()>
                                                    {move || copy_feedback.get()}
                                                </p>
                                            </div>
                                        </div>
                                        <div class="mfa-verify-block">
                                            <p class="section-label">"Step 3 of 3"</p>
                                            <h3>"Enter the 6-digit code"</h3>
                                            <p class="mfa-hint">"Your app refreshes a new code about every 30 seconds."</p>
                                            <label class="auth-field">
                                                <span>"Authentication code"</span>
                                                <input
                                                    class="auth-input mfa-code-input"
                                                    inputmode="numeric"
                                                    autocomplete="one-time-code"
                                                    maxlength="8"
                                                    placeholder="123 456"
                                                    prop:value=move || enroll_code.get()
                                                    on:input=move |event| {
                                                        let raw = event_target_value(&event);
                                                        set_enroll_code.set(raw.chars().filter(|ch| ch.is_ascii_digit()).take(6).collect());
                                                    }
                                                />
                                                <small>"Confirm enrollment before you leave this page."</small>
                                            </label>
                                            <button
                                                type="button"
                                                class="primary-button"
                                                disabled=move || confirm.pending().get() || enroll_code.get().len() < 6
                                                on:click=move |_| {
                                                    confirm.dispatch(ConfirmTotpEnrollment {
                                                        code: enroll_code.get_untracked(),
                                                    });
                                                }
                                            >
                                                {move || if confirm.pending().get() { "Verifying…" } else { "Confirm and enable" }}
                                            </button>
                                            <p class="error-banner" hidden=move || !matches!(confirm.value().get(), Some(Err(_)))>
                                                {move || match confirm.value().get() {
                                                    Some(Err(error)) => server_error_text(error),
                                                    _ => String::new(),
                                                }}
                                            </p>
                                        </div>
                                    </div>
                                </div>
                            </section>
                        </div>
                    }.into_any();
                }

                let enrolled = status
                    .get()
                    .and_then(Result::ok)
                    .is_some_and(|value| value.totp_enrolled);

                // Already enrolled — management tools only
                if enrolled {
                    return view! {
                        <div class="mfa-overview">
                            <section class="panel mfa-status-panel">
                                <div class="mfa-status-head">
                                    <div>
                                        <p class="section-label">"Security factor"</p>
                                        <h2>"Authenticator app (TOTP)"</h2>
                                        <p class="mfa-lede">
                                            "Time-based codes from your authenticator app protect sensitive account actions."
                                        </p>
                                    </div>
                                    <span class="mfa-badge mfa-badge-on">"Enabled"</span>
                                </div>
                                <dl class="kv mfa-status-kv">
                                    <dt>"App codes"</dt>
                                    <dd>"Ready"</dd>
                                    <dt>"Recovery codes left"</dt>
                                    <dd>{move || status.get().and_then(Result::ok).map(|value| value.recovery_codes_remaining.to_string()).unwrap_or_default()}</dd>
                                    <dt>"Session assurance"</dt>
                                    <dd class="mono-value">{move || status.get().and_then(Result::ok).map(|value| value.assurance.to_uppercase()).unwrap_or_default()}</dd>
                                </dl>
                            </section>
                            <section class="panel">
                                <p class="section-label">"This session"</p>
                                <h2>"Step up to AAL2"</h2>
                                <p class="mfa-lede">
                                    "Sensitive actions (like changing your password) may require a fresh authenticator code for this browser session."
                                </p>
                                <label class="auth-field">
                                    <span>"Authentication code"</span>
                                    <input
                                        class="auth-input mfa-code-input"
                                        inputmode="numeric"
                                        autocomplete="one-time-code"
                                        maxlength="8"
                                        placeholder="123 456"
                                        prop:value=move || step_up_code.get()
                                        on:input=move |event| {
                                            let raw = event_target_value(&event);
                                            set_step_up_code.set(raw.chars().filter(|ch| ch.is_ascii_digit()).take(6).collect());
                                        }
                                    />
                                </label>
                                <button
                                    type="button"
                                    class="primary-button"
                                    disabled=move || verify.pending().get() || step_up_code.get().len() < 6
                                    on:click=move |_| {
                                        verify.dispatch(VerifyTotpStepUp {
                                            code: step_up_code.get_untracked(),
                                        });
                                    }
                                >
                                    {move || if verify.pending().get() { "Verifying…" } else { "Verify code" }}
                                </button>
                                <p class="result-line" hidden=move || verify.value().get().is_none()>
                                    {move || action_result_text(verify.value().get())}
                                </p>
                            </section>
                            <section class="panel">
                                <p class="section-label">"Backup"</p>
                                <h2>"Use a recovery code"</h2>
                                <p class="mfa-lede">
                                    "If you cannot open your authenticator app, enter one unused recovery code. That code will be consumed."
                                </p>
                                <label class="auth-field">
                                    <span>"Recovery code"</span>
                                    <input
                                        class="auth-input"
                                        autocomplete="one-time-code"
                                        maxlength="32"
                                        prop:value=move || recovery_code.get()
                                        on:input=move |event| set_recovery_code.set(event_target_value(&event).trim().to_owned())
                                    />
                                </label>
                                <button
                                    type="button"
                                    class="secondary-button"
                                    disabled=move || recover.pending().get() || recovery_code.get().is_empty()
                                    on:click=move |_| {
                                        recover.dispatch(VerifyRecoveryCode {
                                            code: recovery_code.get_untracked(),
                                        });
                                    }
                                >"Use recovery code"</button>
                                <p class="result-line" hidden=move || recover.value().get().is_none()>
                                    {move || action_result_text(recover.value().get())}
                                </p>
                            </section>
                        </div>
                    }.into_any();
                }

                // Default overview: status + setup CTA only
                view! {
                    <div class="mfa-overview">
                        <section class="panel mfa-status-panel">
                            <div class="mfa-status-head">
                                <div>
                                    <p class="section-label">"Security factor"</p>
                                    <h2>"Authenticator app (TOTP)"</h2>
                                    <p class="mfa-lede">
                                        "Use Google Authenticator, 1Password, Authy, or any app that supports time-based one-time passwords."
                                    </p>
                                </div>
                                {move || match status.get() {
                                    Some(Ok(value)) if value.totp_enrolled => view! {
                                        <span class="mfa-badge mfa-badge-on">"Enabled"</span>
                                    }.into_any(),
                                    Some(Ok(_)) => view! {
                                        <span class="mfa-badge mfa-badge-off">"Not enabled"</span>
                                    }.into_any(),
                                    Some(Err(_)) => view! {
                                        <span class="mfa-badge mfa-badge-off">"Unavailable"</span>
                                    }.into_any(),
                                    None => view! {
                                        <span class="mfa-badge mfa-badge-off">"Loading"</span>
                                    }.into_any(),
                                }}
                            </div>
                            <dl class="kv mfa-status-kv" hidden=move || !matches!(status.get(), Some(Ok(_)))>
                                <dt>"App codes"</dt>
                                <dd>{move || status.get().and_then(Result::ok).map(|value| if value.totp_enrolled { "Ready" } else { "Not set up" }).unwrap_or_default()}</dd>
                                <dt>"Recovery codes left"</dt>
                                <dd>{move || status.get().and_then(Result::ok).map(|value| value.recovery_codes_remaining.to_string()).unwrap_or_default()}</dd>
                                <dt>"Session assurance"</dt>
                                <dd class="mono-value">{move || status.get().and_then(Result::ok).map(|value| value.assurance.to_uppercase()).unwrap_or_default()}</dd>
                            </dl>
                            <p class="error-banner" hidden=move || !matches!(status.get(), Some(Err(_)))>
                                {move || match status.get() {
                                    Some(Err(error)) => server_error_text(error),
                                    _ => String::new(),
                                }}
                            </p>
                        </section>
                        <section class="panel">
                            <p class="section-label">"Set up"</p>
                            <h2>"Add an authenticator"</h2>
                            <ol class="mfa-steps-preview">
                                <li><strong>"Install"</strong>" an authenticator app on your phone."</li>
                                <li><strong>"Scan"</strong>" a QR code we show you (or type a secret)."</li>
                                <li><strong>"Enter"</strong>" the 6-digit code the app shows to finish."</li>
                                <li><strong>"Save"</strong>" recovery codes in a safe place — shown once."</li>
                            </ol>
                            <button
                                type="button"
                                class="primary-button"
                                disabled=move || start.pending().get()
                                on:click=move |_| {
                                    set_show_manual_secret.set(false);
                                    set_enroll_code.set(String::new());
                                    set_copy_feedback.set(String::new());
                                    start.dispatch(StartTotpEnrollment {});
                                }
                            >"Set up authenticator"</button>
                            <p class="error-banner" hidden=move || !matches!(start.value().get(), Some(Err(_)))>
                                {move || match start.value().get() {
                                    Some(Err(error)) => server_error_text(error),
                                    _ => String::new(),
                                }}
                            </p>
                        </section>
                    </div>
                }.into_any()
            }}
        </div>
    }
}

fn otpauth_qr_svg(uri: &str) -> String {
    #[cfg(feature = "hydrate")]
    {
        use qrcode::render::svg;
        use qrcode::QrCode;
        match QrCode::new(uri.as_bytes()) {
            Ok(code) => code
                .render::<svg::Color>()
                .min_dimensions(168, 168)
                .dark_color(svg::Color("#0d0d0d"))
                .light_color(svg::Color("#ffffff"))
                .quiet_zone(true)
                .build(),
            Err(_) => String::new(),
        }
    }
    #[cfg(not(feature = "hydrate"))]
    {
        let _ = uri;
        String::new()
    }
}

#[component]
fn AccountSessionsPage() -> impl IntoView {
    account_page_shell(
        "Sessions",
        "Review and revoke browser access for this account.",
        "sessions",
        view! { <AccountSessionManager /> },
    )
}

/// If the user has zero organizations, force Linear-style first-workspace onboarding
/// (except account security + onboarding itself).
///
/// Island must not call `use_location()` — islands hydrate outside the Router context.
#[island]
fn WorkspaceOnboardingGate() -> impl IntoView {
    let orgs = browser_load(list_organizations);
    Effect::new(move |_| {
        let path = current_browser_pathname();
        let path = path.trim_end_matches('/');
        let allow = path.starts_with("/onboarding")
            || path.starts_with("/account")
            || path.starts_with("/auth")
            || path.starts_with("/invitations")
            || path.starts_with("/login")
            || path.starts_with("/register");
        if allow {
            return;
        }
        if let Some(Ok(list)) = orgs.get() {
            if list.organizations.is_empty() {
                redirect_browser("/onboarding/workspace");
            }
        }
    });
    view! { <></> }
}

/// Pathname for island code (no Router context on hydrate).
fn current_browser_pathname() -> String {
    #[cfg(feature = "hydrate")]
    {
        window()
            .and_then(|w| w.location().pathname().ok())
            .unwrap_or_else(|| "/".to_owned())
    }
    #[cfg(not(feature = "hydrate"))]
    {
        "/".to_owned()
    }
}

/// True when URL has `new=1` / `new=true` (create-workspace intent).
fn current_browser_search_has_new() -> bool {
    #[cfg(feature = "hydrate")]
    {
        let search = window()
            .and_then(|w| w.location().search().ok())
            .unwrap_or_default();
        let q = search.trim_start_matches('?');
        q.split('&').any(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next().unwrap_or("");
            let val = parts.next().unwrap_or("1");
            key == "new"
                && matches!(
                    val.to_ascii_lowercase().as_str(),
                    "" | "1" | "true" | "yes" | "on"
                )
        })
    }
    #[cfg(not(feature = "hydrate"))]
    {
        false
    }
}

/// Legacy `/account/vault` → selected org vault or onboarding.
#[component]
fn AccountVaultRedirectPage() -> impl IntoView {
    page_shell(
        "Secret vault",
        "Opening your workspace vault…",
        view! { <AccountVaultRedirect /> },
    )
}

#[island]
fn AccountVaultRedirect() -> impl IntoView {
    let target = browser_load(resolve_workspace_vault_target);
    Effect::new(move |_| match target.get() {
        Some(Ok(org)) if !org.slug.is_empty() => {
            redirect_browser(&format!("/org/{}/vault", org.slug));
        }
        Some(Ok(_)) | Some(Err(_)) => {
            redirect_browser("/onboarding/workspace");
        }
        None => {}
    });
    view! {
        <section class="panel">
            <p class="result-line">"Redirecting to your workspace vault…"</p>
            <p class="board-muted">
                <a href="/onboarding/workspace">"Create a workspace"</a>
                " if you do not have one yet."
            </p>
        </section>
    }
}

#[component]
fn OrgVaultPage() -> impl IntoView {
    let params = use_params_map();
    account_page_shell(
        "Secret vault",
        "Encrypted at rest. Values are never shown in lists. Use keys like STRIPE_SECRET_KEY in connectors.",
        "vault",
        view! {
            {move || {
                let slug = params
                    .get()
                    .get("slug")
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                view! { <AccountVaultPanel org_slug=slug /> }.into_any()
            }}
        },
    )
}

#[component]
fn WorkspaceOnboardingPage() -> impl IntoView {
    // Minimal chrome — Linear-style focused create.
    view! {
        <div class="page onboarding-page">
            <header class="page-brand">
                <a class="page-brand-link" href="/" aria-label="wasi-auth home">
                    <span class="page-brand-mark" aria-hidden="true">"d"</span>
                    <span>
                        <strong>"wasi-auth"</strong>
                        <small>"Create your workspace"</small>
                    </span>
                </a>
            </header>
            <WorkspaceOnboardingPanel />
        </div>
    }
}

#[island]
fn WorkspaceOnboardingPanel() -> impl IntoView {
    let create_action = ServerAction::<CreateOrganization>::new();
    let pending = create_action.pending();
    let value = create_action.value();
    let name = RwSignal::new(String::new());
    let slug = RwSignal::new(String::new());
    let slug_touched = RwSignal::new(false);
    let client_error = RwSignal::new(None::<String>);

    // Force-create intent: /onboarding/workspace?new=1 (from workspace switcher).
    // Without new=1, users who already have orgs are sent to their workspace (first-time gate only).
    let force_new = current_browser_search_has_new();

    let existing = browser_load(list_organizations);
    Effect::new(move |_| {
        if force_new {
            return;
        }
        if let Some(Ok(list)) = existing.get() {
            if list.organizations.is_empty() {
                return;
            }
            if let Some(org) = list.organizations.iter().find(|o| !o.slug.is_empty()) {
                redirect_browser(&format!("/org/{}/vault", org.slug));
            } else {
                redirect_browser("/organizations");
            }
        }
    });

    Effect::new(move |_| match value.get() {
        Some(Ok(org)) => {
            let dest = if org.slug.is_empty() {
                "/dashboard".to_owned()
            } else {
                format!("/org/{}/vault", org.slug)
            };
            redirect_browser(&dest);
        }
        Some(Err(e)) => client_error.set(Some(e.to_string())),
        None => {}
    });

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

    view! {
        <section class="panel onboarding-card">
            <p class="section-label">"Welcome"</p>
            <h1 class="onboarding-title">"Create your workspace"</h1>
            <p class="onboarding-lede">
                "Workspaces hold your team, secret vault, and connectors. "
                "Pick a name and a short URL — you can invite others later."
            </p>
            <div class="onboarding-form">
                <label class="auth-field">
                    <span>"Workspace name"</span>
                    <input
                        class="auth-input"
                        type="text"
                        maxlength="120"
                        placeholder="Acme Inc"
                        prop:value=move || name.get()
                        on:input=move |e| {
                            let v = event_target_value(&e);
                            name.set(v.clone());
                            if !slug_touched.get_untracked() {
                                slug.set(derive_slug(&v));
                            }
                            client_error.set(None);
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
                            placeholder="acme"
                            prop:value=move || slug.get()
                            on:input=move |e| {
                                slug_touched.set(true);
                                slug.set(derive_slug(&event_target_value(&e)));
                                client_error.set(None);
                            }
                        />
                    </div>
                    <span class="board-muted">"Used in links like /org/acme/vault. Letters, numbers, hyphens."</span>
                </label>
                <button
                    type="button"
                    class="primary-button"
                    disabled=move || pending.get() || name.get().trim().is_empty() || slug.get().trim().len() < 2
                    on:click=move |_| {
                        create_action.dispatch(CreateOrganization {
                            name: name.get_untracked().trim().to_owned(),
                            slug: slug.get_untracked().trim().to_owned(),
                        });
                    }
                >
                    {move || if pending.get() { "Creating…" } else { "Create workspace" }}
                </button>
                <p class="error-banner" hidden=move || client_error.get().is_none()>
                    {move || client_error.get().unwrap_or_default()}
                </p>
            </div>
        </section>
    }
}

#[island]
fn AccountVaultPanel(org_slug: String) -> impl IntoView {
    let org_slug = RwSignal::new(org_slug);
    let org_slug_for_load = org_slug.get_untracked();
    let secrets_res = browser_load(move || list_dashboard_secrets(org_slug_for_load.clone()));
    let secrets = RwSignal::new(Vec::<crate::contracts::SecretSummary>::new());
    let form_error = RwSignal::new(None::<String>);
    let form_ok = RwSignal::new(None::<String>);
    let key = RwSignal::new(String::new());
    let label = RwSignal::new(String::new());
    let description = RwSignal::new(String::new());
    let value = RwSignal::new(String::new());
    let show_value = RwSignal::new(false);
    let create_open = RwSignal::new(false);
    let delete_target = RwSignal::new(None::<(String, String)>); // (id, key)
    let revealed = RwSignal::new(std::collections::HashMap::<String, String>::new());
    let reveal_pending = RwSignal::new(None::<String>);

    let create_action = ServerAction::<CreateDashboardSecret>::new();
    let delete_action = ServerAction::<DeleteDashboardSecret>::new();
    let reveal_action = ServerAction::<RevealDashboardSecret>::new();
    let seed_action = ServerAction::<SeedDashboardDemos>::new();

    Effect::new(move |_| {
        if let Some(Ok(list)) = secrets_res.get() {
            secrets.set(list);
        } else if let Some(Err(e)) = secrets_res.get() {
            form_error.set(Some(e.to_string()));
        }
    });

    Effect::new(move |_| match create_action.value().get() {
        Some(Ok(summary)) => {
            secrets.update(|list| {
                if !list.iter().any(|s| s.id == summary.id) {
                    list.push(summary);
                }
            });
            key.set(String::new());
            label.set(String::new());
            description.set(String::new());
            value.set(String::new());
            show_value.set(false);
            create_open.set(false);
            form_error.set(None);
            form_ok.set(Some("Secret stored. Value is encrypted and hidden.".into()));
        }
        Some(Err(e)) => {
            form_ok.set(None);
            form_error.set(Some(e.to_string()));
        }
        None => {}
    });

    Effect::new(move |_| match delete_action.value().get() {
        Some(Ok(_)) => {
            if let Some((id, _)) = delete_target.get_untracked() {
                secrets.update(|l| l.retain(|s| s.id != id));
                revealed.update(|m| {
                    m.remove(&id);
                });
            }
            delete_target.set(None);
            form_error.set(None);
            form_ok.set(Some("Secret deleted.".into()));
        }
        Some(Err(e)) => {
            form_ok.set(None);
            form_error.set(Some(e.to_string()));
        }
        None => {}
    });

    Effect::new(move |_| match reveal_action.value().get() {
        Some(Ok(resp)) => {
            let ttl = resp.reveal_ttl_seconds.max(5) as u64;
            revealed.update(|map| {
                map.insert(resp.id.clone(), resp.value);
            });
            reveal_pending.set(None);
            form_error.set(None);
            #[cfg(feature = "hydrate")]
            {
                let id_hide = resp.id.clone();
                spawn_local(async move {
                    gloo_timers_sleep_ms(ttl.saturating_mul(1000)).await;
                    revealed.update(|map| {
                        map.remove(&id_hide);
                    });
                });
            }
            let _ = ttl;
        }
        Some(Err(e)) => {
            reveal_pending.set(None);
            let msg = e.to_string();
            if msg.to_ascii_lowercase().contains("forbidden")
                || msg.to_ascii_lowercase().contains("step")
            {
                form_error.set(Some(
                    "Reveal requires step-up (AAL2). Complete MFA on /account/mfa, then try again."
                        .into(),
                ));
            } else {
                form_error.set(Some(msg));
            }
        }
        None => {}
    });

    Effect::new(move |_| {
        if let Some(Ok(_)) = seed_action.value().get() {
            form_ok.set(Some(
                "Demo connectors seeded. Open the dashboard to see bound widgets.".into(),
            ));
            form_error.set(None);
            #[cfg(feature = "hydrate")]
            {
                let slug = org_slug.get_untracked();
                spawn_local(async move {
                    if let Ok(list) = list_dashboard_secrets(slug).await {
                        secrets.set(list);
                    }
                });
            }
        }
    });

    let open_create_modal = move || {
        form_error.set(None);
        form_ok.set(None);
        key.set(String::new());
        label.set(String::new());
        description.set(String::new());
        value.set(String::new());
        show_value.set(false);
        create_open.set(true);
    };

    view! {
        <div class="vault-page">
            <section class="panel vault-intro">
                <p class="section-label">"Connectors · Integrations"</p>
                <p class="vault-lede">
                    "Store API keys and passwords for REST, Postgres, and future integrations. "
                    "Resource pickers reference keys by id — plaintext never appears in list APIs."
                </p>
                <div class="vault-actions">
                    <a class="secondary-button" href="/dashboard">"Back to dashboard"</a>
                    <button
                        type="button"
                        class="secondary-button"
                        disabled=move || seed_action.pending().get()
                        on:click=move |_| { seed_action.dispatch(SeedDashboardDemos {}); }
                    >
                        {move || if seed_action.pending().get() { "Loading demos…" } else { "Load demo connectors" }}
                    </button>
                </div>
            </section>

            <p class="error-banner" hidden=move || form_error.get().is_none()>
                {move || form_error.get().unwrap_or_default()}
            </p>
            <p class="success-banner" hidden=move || form_ok.get().is_none()>
                {move || form_ok.get().unwrap_or_default()}
            </p>

            <section class="panel vault-list-panel">
                <header class="vault-panel-head">
                    <h2>"Secrets"</h2>
                    <div class="vault-panel-head-meta">
                        <span class="board-muted">{move || format!("{} stored", secrets.get().len())}</span>
                        <button type="button" class="secondary-button vault-add-inline" on:click=move |_| open_create_modal()>
                            "Add secret"
                        </button>
                    </div>
                </header>
                <div class="vault-table-wrap">
                    <table class="vault-table">
                        <colgroup>
                            <col class="vault-col-key" />
                            <col class="vault-col-label" />
                            <col class="vault-col-scope" />
                            <col class="vault-col-value" />
                            <col class="vault-col-actions" />
                        </colgroup>
                        <thead>
                            <tr>
                                <th scope="col">"Key"</th>
                                <th scope="col">"Label"</th>
                                <th scope="col">"Scope"</th>
                                <th scope="col">"Value"</th>
                                <th scope="col" class="vault-th-actions"><span class="sr-only">"Actions"</span></th>
                            </tr>
                        </thead>
                        <tbody>
                            {move || {
                                let list = secrets.get();
                                if list.is_empty() {
                                    return view! {
                                        <tr>
                                            <td colspan="5" class="board-muted vault-empty">
                                                "No secrets yet. Use Add secret above."
                                            </td>
                                        </tr>
                                    }.into_any();
                                }
                                list.into_iter().map(|sec| {
                                    let id = sec.id.clone();
                                    let id_for_reveal = sec.id.clone();
                                    let id_for_pending = sec.id.clone();
                                    let id_del = sec.id.clone();
                                    let key_label = if sec.key.is_empty() { sec.name.clone() } else { sec.key.clone() };
                                    let key_for_delete = key_label.clone();
                                    let label_text = sec.label.clone();
                                    let scope = sec.scope.clone();
                                    let masked = sec.masked_value.clone();
                                    view! {
                                        <tr>
                                            <td class="mono-value vault-td-key">{key_label}</td>
                                            <td class="vault-td-label">{label_text}</td>
                                            <td class="vault-td-scope"><span class="vault-scope">{scope}</span></td>
                                            <td class="vault-td-value">
                                                <div class="vault-value-inner">
                                                    {move || {
                                                        let id_check = id.clone();
                                                        let revealed_map = revealed.get();
                                                        if let Some(plain) = revealed_map.get(&id_check).cloned() {
                                                            view! {
                                                                <code class="vault-revealed">{plain}</code>
                                                            }.into_any()
                                                        } else {
                                                            let id_click = id_for_reveal.clone();
                                                            let id_pend = id_for_pending.clone();
                                                            let masked_show = masked.clone();
                                                            view! {
                                                                <span class="vault-masked">{masked_show}</span>
                                                                <button
                                                                    type="button"
                                                                    class="vault-eye"
                                                                    aria-label="Reveal secret"
                                                                    disabled={
                                                                        let id_pend = id_pend.clone();
                                                                        move || {
                                                                            reveal_pending.get().as_ref() == Some(&id_pend)
                                                                                || reveal_action.pending().get()
                                                                        }
                                                                    }
                                                                    on:click=move |_| {
                                                                        reveal_pending.set(Some(id_click.clone()));
                                                                        reveal_action.dispatch(RevealDashboardSecret {
                                                                            org_slug: org_slug.get_untracked(),
                                                                            secret_id: id_click.clone(),
                                                                        });
                                                                    }
                                                                >"👁"</button>
                                                            }.into_any()
                                                        }
                                                    }}
                                                </div>
                                            </td>
                                            <td class="vault-td-actions">
                                                <button
                                                    type="button"
                                                    class="vault-trash"
                                                    aria-label=format!("Delete secret {key_for_delete}")
                                                    title="Delete secret"
                                                    on:click=move |_| {
                                                        form_ok.set(None);
                                                        delete_target.set(Some((id_del.clone(), key_for_delete.clone())));
                                                    }
                                                >
                                                    <svg class="vault-trash-icon" viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">
                                                        <path d="M3 6h18" />
                                                        <path d="M8 6V4h8v2" />
                                                        <path d="M19 6l-1 14H6L5 6" />
                                                        <path d="M10 11v6" />
                                                        <path d="M14 11v6" />
                                                    </svg>
                                                </button>
                                            </td>
                                        </tr>
                                    }
                                }).collect_view().into_any()
                            }}
                        </tbody>
                    </table>
                </div>
            </section>

            // Create secret modal
            <Show when=move || create_open.get()>
                <div
                    class="board-modal-backdrop vault-modal-backdrop"
                    role="presentation"
                    on:click=move |_| create_open.set(false)
                    on:wheel=move |e| e.stop_propagation()
                >
                    <div
                        class="board-modal vault-modal"
                        role="dialog"
                        aria-modal="true"
                        aria-labelledby="vault-create-title"
                        on:click=move |e| e.stop_propagation()
                    >
                        <header class="board-modal-head">
                            <div>
                                <h2 id="vault-create-title">"Add secret"</h2>
                                <p>"Keys look like environment variables. Values are encrypted with AUTH_VAULT_KEY before storage."</p>
                            </div>
                            <button type="button" class="board-modal-close" on:click=move |_| create_open.set(false)>"Close"</button>
                        </header>
                        <div class="board-modal-body vault-modal-body">
                            <div class="vault-form vault-form-modal">
                                <label class="auth-field">
                                    <span>"Key"</span>
                                    <input
                                        class="auth-input mono-value"
                                        placeholder="STRIPE_SECRET_KEY"
                                        prop:value=move || key.get()
                                        on:input=move |e| key.set(event_target_value(&e).to_ascii_uppercase())
                                    />
                                </label>
                                <label class="auth-field">
                                    <span>"Label"</span>
                                    <input
                                        class="auth-input"
                                        placeholder="Stripe live secret"
                                        prop:value=move || label.get()
                                        on:input=move |e| label.set(event_target_value(&e))
                                    />
                                </label>
                                <label class="auth-field vault-field-wide">
                                    <span>"Description (optional)"</span>
                                    <input
                                        class="auth-input"
                                        prop:value=move || description.get()
                                        on:input=move |e| description.set(event_target_value(&e))
                                    />
                                </label>
                                <label class="auth-field vault-field-wide">
                                    <span>"Value"</span>
                                    <div class="vault-value-input-row">
                                        <input
                                            class="auth-input"
                                            type=move || if show_value.get() { "text" } else { "password" }
                                            autocomplete="new-password"
                                            prop:value=move || value.get()
                                            on:input=move |e| value.set(event_target_value(&e))
                                        />
                                        <button
                                            type="button"
                                            class="secondary-button"
                                            on:click=move |_| show_value.update(|v| *v = !*v)
                                        >
                                            {move || if show_value.get() { "Hide" } else { "Show" }}
                                        </button>
                                    </div>
                                </label>
                            </div>
                            <div class="vault-modal-actions">
                                <button type="button" class="secondary-button" on:click=move |_| create_open.set(false)>"Cancel"</button>
                                <button
                                    type="button"
                                    class="primary-button"
                                    disabled=move || create_action.pending().get()
                                    on:click=move |_| {
                                        form_ok.set(None);
                                        form_error.set(None);
                                        create_action.dispatch(CreateDashboardSecret {
                                            org_slug: org_slug.get_untracked(),
                                            request: SecretCreateRequest {
                                                key: key.get_untracked(),
                                                name: key.get_untracked(),
                                                value: value.get_untracked(),
                                                label: label.get_untracked(),
                                                description: description.get_untracked(),
                                                scope: "user".to_owned(),
                                            },
                                        });
                                    }
                                >
                                    {move || if create_action.pending().get() { "Encrypting…" } else { "Store secret" }}
                                </button>
                            </div>
                        </div>
                    </div>
                </div>
            </Show>

            // Delete confirmation modal
            <Show when=move || delete_target.get().is_some()>
                <div
                    class="board-modal-backdrop vault-modal-backdrop"
                    role="presentation"
                    on:click=move |_| delete_target.set(None)
                    on:wheel=move |e| e.stop_propagation()
                >
                    <div
                        class="board-modal vault-modal vault-modal-confirm"
                        role="dialog"
                        aria-modal="true"
                        aria-labelledby="vault-delete-title"
                        on:click=move |e| e.stop_propagation()
                    >
                        <header class="board-modal-head">
                            <div>
                                <h2 id="vault-delete-title">"Delete secret?"</h2>
                                <p>
                                    "This cannot be undone. Resources using "
                                    <strong class="mono-value">{move || delete_target.get().map(|(_, k)| k).unwrap_or_default()}</strong>
                                    " will fail until reconfigured."
                                </p>
                            </div>
                            <button type="button" class="board-modal-close" on:click=move |_| delete_target.set(None)>"Close"</button>
                        </header>
                        <div class="board-modal-body">
                            <div class="vault-modal-actions">
                                <button type="button" class="secondary-button" on:click=move |_| delete_target.set(None)>"Cancel"</button>
                                <button
                                    type="button"
                                    class="primary-button vault-danger-button"
                                    disabled=move || delete_action.pending().get()
                                    on:click=move |_| {
                                        if let Some((id, _)) = delete_target.get_untracked() {
                                            delete_action.dispatch(DeleteDashboardSecret {
                                                org_slug: org_slug.get_untracked(),
                                                secret_id: id,
                                            });
                                        }
                                    }
                                >
                                    {move || if delete_action.pending().get() { "Deleting…" } else { "Delete secret" }}
                                </button>
                            </div>
                        </div>
                    </div>
                </div>
            </Show>
        </div>
    }
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

#[island]
fn AccountProfileCard() -> impl IntoView {
    let profile = browser_load(get_account_profile);
    let action = ServerAction::<UpdateAccountProfile>::new();
    let pending = action.pending();
    let value = action.value();

    let (first_name, set_first_name) = signal(String::new());
    let (last_name, set_last_name) = signal(String::new());
    let (display_name, set_display_name) = signal(String::new());
    let (username, set_username) = signal(String::new());
    let (is_public, set_is_public) = signal(false);
    let (avatar_data_url, set_avatar_data_url) = signal(None::<String>);
    let (avatar_dirty, set_avatar_dirty) = signal(false);
    let (client_error, set_client_error) = signal(None::<String>);
    let (seeded, set_seeded) = signal(false);

    Effect::new(move |_| {
        if seeded.get() {
            return;
        }
        // Prefer a successful save result so the form stays in sync after update.
        if let Some(Ok(saved)) = value.get() {
            seed_profile_form(
                &saved,
                set_first_name,
                set_last_name,
                set_display_name,
                set_username,
                set_is_public,
                set_avatar_data_url,
            );
            set_avatar_dirty.set(false);
            set_seeded.set(true);
            return;
        }
        if let Some(Ok(loaded)) = profile.get() {
            seed_profile_form(
                &loaded,
                set_first_name,
                set_last_name,
                set_display_name,
                set_username,
                set_is_public,
                set_avatar_data_url,
            );
            set_avatar_dirty.set(false);
            set_seeded.set(true);
        }
    });

    // After a successful save, re-seed from the response.
    Effect::new(move |_| {
        if let Some(Ok(saved)) = value.get() {
            seed_profile_form(
                &saved,
                set_first_name,
                set_last_name,
                set_display_name,
                set_username,
                set_is_public,
                set_avatar_data_url,
            );
            set_avatar_dirty.set(false);
            set_client_error.set(None);
        }
    });

    let preview_initials = move || {
        profile_initials(
            &display_name.get(),
            &first_name.get(),
            &last_name.get(),
            profile
                .get()
                .and_then(Result::ok)
                .and_then(|p| p.email)
                .as_deref()
                .unwrap_or(""),
        )
    };

    let on_avatar_file = move |event| {
        #[cfg(feature = "hydrate")]
        {
            use wasm_bindgen::JsCast;
            let input: web_sys::HtmlInputElement = event_target(&event);
            spawn_local(async move {
                match pick_image_data_url(input, 250_000).await {
                    Ok(value) if value.is_null() || value.is_undefined() => {}
                    Ok(value) => {
                        if let Some(data_url) = value.as_string() {
                            set_avatar_data_url.set(Some(data_url));
                            set_avatar_dirty.set(true);
                            set_client_error.set(None);
                        }
                    }
                    Err(error) => {
                        let message = error
                            .as_string()
                            .unwrap_or_else(|| "Could not read image.".to_owned());
                        set_client_error.set(Some(message));
                    }
                }
            });
        }
        #[cfg(not(feature = "hydrate"))]
        {
            let _ = event;
        }
    };

    view! {
        <section class="panel profile-editor">
            {move || match profile.get() {
                Some(Err(error)) => view! {
                    <p class="error-banner">{server_error_text(error)}</p>
                }.into_any(),
                None if !seeded.get() => view! {
                    <div class="profile-loading" aria-busy="true">
                        <div class="profile-skeleton-avatar"></div>
                        <div class="profile-skeleton-lines">
                            <span></span><span></span><span></span>
                        </div>
                    </div>
                }.into_any(),
                _ => view! {
                    <div class="profile-editor-body">
                        // Centered identity: avatar + one primary line + optional handle
                        <header class="profile-identity-strip">
                            <div
                                class="profile-avatar-wrap"
                                class:has-photo=move || {
                                    avatar_data_url
                                        .get()
                                        .as_ref()
                                        .is_some_and(|url| !url.is_empty())
                                }
                            >
                                <Show when=move || {
                                    avatar_data_url
                                        .get()
                                        .as_ref()
                                        .is_some_and(|url| !url.is_empty())
                                }>
                                    <button
                                        type="button"
                                        class="profile-avatar-clear"
                                        aria-label="Remove photo"
                                        title="Remove photo"
                                        on:click=move |ev| {
                                            ev.prevent_default();
                                            ev.stop_propagation();
                                            set_avatar_data_url.set(None);
                                            set_avatar_dirty.set(true);
                                            set_client_error.set(None);
                                        }
                                    >
                                        <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
                                            <path
                                                fill="currentColor"
                                                d="M3.72 3.72a.75.75 0 0 1 1.06 0L8 6.94l3.22-3.22a.75.75 0 1 1 1.06 1.06L9.06 8l3.22 3.22a.75.75 0 1 1-1.06 1.06L8 9.06l-3.22 3.22a.75.75 0 0 1-1.06-1.06L6.94 8 3.72 4.78a.75.75 0 0 1 0-1.06Z"
                                            />
                                        </svg>
                                    </button>
                                </Show>
                                <label class="profile-avatar-control" title="Change photo">
                                    <input
                                        type="file"
                                        accept="image/png,image/jpeg,image/webp,image/gif"
                                        class="profile-file-input"
                                        aria-label="Upload profile photo"
                                        on:change=on_avatar_file
                                    />
                                    <span class="profile-avatar-disk" aria-hidden="true">
                                        {move || match avatar_data_url.get() {
                                            Some(url) if !url.is_empty() => view! {
                                                <img class="profile-avatar-img" src=url alt="" />
                                            }.into_any(),
                                            _ => view! {
                                                <span class="profile-avatar-fallback">{preview_initials()}</span>
                                            }.into_any(),
                                        }}
                                        <span class="profile-avatar-veil">
                                            <svg class="profile-avatar-camera" viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
                                                <path
                                                    fill="currentColor"
                                                    d="M9 3.75A1.75 1.75 0 0 1 10.53 2.5h2.94A1.75 1.75 0 0 1 15 3.75V5h2.25A2.75 2.75 0 0 1 20 7.75v9.5A2.75 2.75 0 0 1 17.25 20H6.75A2.75 2.75 0 0 1 4 17.25v-9.5A2.75 2.75 0 0 1 6.75 5H9V3.75Zm1.5 1.5V5h3V5.25h-3ZM12 9a4 4 0 1 0 0 8 4 4 0 0 0 0-8Zm0 1.5a2.5 2.5 0 1 1 0 5 2.5 2.5 0 0 1 0-5Z"
                                                />
                                            </svg>
                                        </span>
                                    </span>
                                </label>
                            </div>
                            <div class="profile-identity-copy">
                                <h2 class="profile-display-preview">
                                    {move || {
                                        let display = display_name.get();
                                        let first = first_name.get();
                                        let last = last_name.get();
                                        let composed = format!("{first} {last}").trim().to_owned();
                                        let email = profile
                                            .get()
                                            .and_then(Result::ok)
                                            .and_then(|p| p.email)
                                            .unwrap_or_default();
                                        if !display.trim().is_empty() {
                                            display
                                        } else if !composed.is_empty() {
                                            composed
                                        } else if !email.is_empty() {
                                            email
                                        } else {
                                            "Your name".to_owned()
                                        }
                                    }}
                                </h2>
                                // Handle only when set — never show a placeholder @handle.
                                <Show when=move || !username.get().trim().is_empty()>
                                    <p class="profile-handle-preview">
                                        {move || format!("@{}", username.get().trim().to_ascii_lowercase())}
                                    </p>
                                </Show>
                                // Email only when primary title is a name (avoid duplicate email lines).
                                <Show when=move || {
                                    let display = display_name.get();
                                    let first = first_name.get();
                                    let last = last_name.get();
                                    let composed = format!("{first} {last}").trim().to_owned();
                                    let has_name = !display.trim().is_empty() || !composed.is_empty();
                                    has_name && profile.get().and_then(Result::ok).and_then(|p| p.email).is_some()
                                }>
                                    <p class="profile-email-line">
                                        {move || profile
                                            .get()
                                            .and_then(Result::ok)
                                            .and_then(|p| p.email)
                                            .unwrap_or_default()}
                                    </p>
                                </Show>
                            </div>
                        </header>

                        <div class="profile-sections">
                            <section class="profile-section">
                                <div class="profile-section-head">
                                    <h3>"Name"</h3>
                                    <p>"Legal name stays private unless you publish your profile."</p>
                                </div>
                                <div class="auth-fields profile-form-grid">
                                    <label class="auth-field">
                                        <span>"First name"</span>
                                        <input
                                            class="auth-input"
                                            type="text"
                                            autocomplete="given-name"
                                            maxlength="60"
                                            prop:value=move || first_name.get()
                                            on:input=move |event| {
                                                set_client_error.set(None);
                                                set_first_name.set(event_target_value(&event));
                                            }
                                        />
                                    </label>
                                    <label class="auth-field">
                                        <span>"Last name"</span>
                                        <input
                                            class="auth-input"
                                            type="text"
                                            autocomplete="family-name"
                                            maxlength="60"
                                            prop:value=move || last_name.get()
                                            on:input=move |event| {
                                                set_client_error.set(None);
                                                set_last_name.set(event_target_value(&event));
                                            }
                                        />
                                    </label>
                                    <label class="auth-field profile-field-span">
                                        <span>"Display name"</span>
                                        <input
                                            class="auth-input"
                                            type="text"
                                            autocomplete="nickname"
                                            maxlength="80"
                                            prop:value=move || display_name.get()
                                            on:input=move |event| {
                                                set_client_error.set(None);
                                                set_display_name.set(event_target_value(&event));
                                            }
                                        />
                                        <small>"Shown publicly. Falls back to first + last when empty."</small>
                                    </label>
                                </div>
                            </section>

                            <section class="profile-section">
                                <div class="profile-section-head">
                                    <h3>"Handle"</h3>
                                    <p>"Your unique @username. Required for a public profile link."</p>
                                </div>
                                <div class="auth-fields">
                                    <label class="auth-field">
                                        <span>"Username"</span>
                                        <div class="profile-username-field">
                                            <span class="profile-username-at" aria-hidden="true">"@"</span>
                                            <input
                                                class="auth-input profile-username-input"
                                                type="text"
                                                autocomplete="username"
                                                spellcheck="false"
                                                maxlength="30"
                                                prop:value=move || username.get()
                                                on:input=move |event| {
                                                    set_client_error.set(None);
                                                    let raw = event_target_value(&event);
                                                    let cleaned = raw
                                                        .chars()
                                                        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
                                                        .collect::<String>()
                                                        .to_ascii_lowercase();
                                                    set_username.set(cleaned);
                                                }
                                            />
                                        </div>
                                        <small>"3–30 characters · letters, numbers, underscore"</small>
                                    </label>
                                </div>
                            </section>

                            <section class="profile-section profile-section-privacy">
                                <div class="profile-section-head">
                                    <h3>"Visibility"</h3>
                                    <p>"Profiles are private until you choose to publish."</p>
                                </div>
                                <label class="profile-switch">
                                    <input
                                        type="checkbox"
                                        role="switch"
                                        prop:checked=move || is_public.get()
                                        on:change=move |event| {
                                            set_client_error.set(None);
                                            set_is_public.set(event_target_checked(&event));
                                        }
                                    />
                                    <span class="profile-switch-track" aria-hidden="true">
                                        <span class="profile-switch-thumb"></span>
                                    </span>
                                    <span class="profile-switch-copy">
                                        <strong>"Public profile"</strong>
                                        <small>
                                            {move || if is_public.get() {
                                                "Anyone with your link can see your name, @handle, and photo."
                                            } else {
                                                "Only you can see this profile."
                                            }}
                                        </small>
                                    </span>
                                </label>
                                <Show when=move || {
                                    is_public.get() && !username.get().trim().is_empty()
                                }>
                                    <p class="profile-public-link">
                                        <span class="profile-public-link-label">"Live at"</span>
                                        <a
                                            class="profile-public-link-url"
                                            href=move || format!(
                                                "/u/{}",
                                                username.get().trim().to_ascii_lowercase()
                                            )
                                        >
                                            {move || format!(
                                                "/u/{}",
                                                username.get().trim().to_ascii_lowercase()
                                            )}
                                        </a>
                                    </p>
                                </Show>
                            </section>
                        </div>

                        <footer class="profile-footer">
                            <button
                                type="button"
                                class="primary-button"
                                disabled=move || pending.get()
                                on:click=move |_| {
                                    set_client_error.set(None);
                                    let handle = username.get_untracked().trim().to_owned();
                                    if !handle.is_empty()
                                        && (handle.len() < 3 || handle.len() > 30)
                                    {
                                        set_client_error.set(Some(
                                            "Username must be 3–30 characters.".to_owned(),
                                        ));
                                        return;
                                    }
                                    action.dispatch(UpdateAccountProfile {
                                        first_name: first_name.get_untracked(),
                                        last_name: last_name.get_untracked(),
                                        display_name: display_name.get_untracked(),
                                        username: handle,
                                        is_public: is_public.get_untracked(),
                                        avatar_data_url: if avatar_dirty.get_untracked() {
                                            Some(
                                                avatar_data_url
                                                    .get_untracked()
                                                    .unwrap_or_default(),
                                            )
                                        } else {
                                            None
                                        },
                                    });
                                }
                            >
                                {move || if pending.get() { "Saving…" } else { "Save changes" }}
                            </button>
                            <p class="error-banner" hidden=move || client_error.get().is_none()>
                                {move || client_error.get().unwrap_or_default()}
                            </p>
                            <Show when=move || {
                                value.get().is_some_and(|result| result.is_err())
                            }>
                                <p class="error-banner">
                                    {move || action_result_text(value.get())}
                                </p>
                            </Show>
                            <Show when=move || matches!(value.get(), Some(Ok(_)))>
                                <p class="auth-success profile-save-ok">
                                    <span>"Saved"</span>
                                </p>
                            </Show>
                        </footer>
                    </div>
                }.into_any(),
            }}
        </section>
    }
}

fn seed_profile_form(
    profile: &ProfileView,
    set_first_name: WriteSignal<String>,
    set_last_name: WriteSignal<String>,
    set_display_name: WriteSignal<String>,
    set_username: WriteSignal<String>,
    set_is_public: WriteSignal<bool>,
    set_avatar_data_url: WriteSignal<Option<String>>,
) {
    set_first_name.set(profile.first_name.clone());
    set_last_name.set(profile.last_name.clone());
    set_display_name.set(profile.display_name.clone());
    set_username.set(profile.username.clone());
    set_is_public.set(profile.is_public);
    set_avatar_data_url.set(profile.avatar_data_url.clone());
}

fn profile_initials(display_name: &str, first: &str, last: &str, email: &str) -> String {
    let display = display_name.trim();
    if !display.is_empty() {
        let parts: Vec<&str> = display.split_whitespace().collect();
        if parts.len() >= 2 {
            let a = parts[0].chars().next().unwrap_or('?');
            let b = parts[1].chars().next().unwrap_or('?');
            return format!(
                "{}{}",
                a.to_ascii_uppercase(),
                b.to_ascii_uppercase()
            );
        }
        return display
            .chars()
            .take(2)
            .map(|c| c.to_ascii_uppercase())
            .collect();
    }
    let first = first.trim();
    let last = last.trim();
    match (first.chars().next(), last.chars().next()) {
        (Some(a), Some(b)) => format!("{}{}", a.to_ascii_uppercase(), b.to_ascii_uppercase()),
        (Some(a), None) => a.to_ascii_uppercase().to_string(),
        (None, Some(b)) => b.to_ascii_uppercase().to_string(),
        _ => account_initials(email),
    }
}

fn account_initials(email: &str) -> String {
    let local = email.split('@').next().unwrap_or(email).trim();
    let mut chars = local.chars().filter(|c| c.is_alphanumeric());
    match (chars.next(), chars.next()) {
        (Some(a), Some(b)) => format!("{}{}", a.to_ascii_uppercase(), b.to_ascii_uppercase()),
        (Some(a), None) => a.to_ascii_uppercase().to_string(),
        _ => "?".to_string(),
    }
}

#[island]
fn PublicProfileCard(handle: String) -> impl IntoView {
    let profile = {
        let handle = handle.clone();
        browser_load(move || get_public_profile(handle))
    };

    view! {
        <section class="panel public-profile-panel">
            {move || match profile.get() {
                None => view! { <p class="result-line">"Loading profile…"</p> }.into_any(),
                Some(Err(_)) => view! {
                    <div class="public-profile-empty">
                        <div class="profile-avatar-fallback public-profile-empty-avatar" aria-hidden="true">"?"</div>
                        <h2>"Profile unavailable"</h2>
                        <p class="result-line">
                            "This @handle is private or does not exist."
                        </p>
                        <a class="link-button" href="/">"Back home"</a>
                    </div>
                }.into_any(),
                Some(Ok(view)) => {
                    let display = if !view.display_name.trim().is_empty() {
                        view.display_name.clone()
                    } else {
                        let composed = format!("{} {}", view.first_name, view.last_name)
                            .trim()
                            .to_owned();
                        if composed.is_empty() {
                            format!("@{}", view.username)
                        } else {
                            composed
                        }
                    };
                    let initials = profile_initials(
                        &view.display_name,
                        &view.first_name,
                        &view.last_name,
                        &view.username,
                    );
                    let handle_label = format!("@{}", view.username);
                    let avatar = view.avatar_data_url.clone();
                    let legal_name = {
                        let composed = format!("{} {}", view.first_name, view.last_name)
                            .trim()
                            .to_owned();
                        if composed.is_empty() || composed == display {
                            None
                        } else {
                            Some(composed)
                        }
                    };
                    view! {
                        <div class="public-profile-hero">
                            <div class="public-profile-avatar" aria-hidden="true">
                                {match avatar {
                                    Some(url) if !url.is_empty() => view! {
                                        <img class="profile-avatar-img" src=url alt="" />
                                    }.into_any(),
                                    _ => view! {
                                        <span class="profile-avatar-fallback">{initials}</span>
                                    }.into_any(),
                                }}
                            </div>
                            <div class="public-profile-meta">
                                <p class="profile-kicker">"Public profile"</p>
                                <h2>{display}</h2>
                                <p class="profile-handle-preview">{handle_label}</p>
                                {legal_name.map(|name| view! {
                                    <p class="profile-email-line">{name}</p>
                                })}
                            </div>
                        </div>
                    }.into_any()
                }
            }}
        </section>
    }
}

#[island]
fn ChangePasswordForm() -> impl IntoView {
    let action = ServerAction::<ChangePassword>::new();
    let pending = action.pending();
    let value = action.value();
    let (current_password, set_current_password) = signal(String::new());
    let (new_password, set_new_password) = signal(String::new());
    let (confirm_password, set_confirm_password) = signal(String::new());
    let (client_error, set_client_error) = signal(None::<String>);

    let can_submit = move || {
        let current = current_password.get();
        let next = new_password.get();
        let confirm = confirm_password.get();
        !pending.get()
            && !current.is_empty()
            && next.chars().count() >= 15
            && next == confirm
            && next != current
    };

    view! {
        <section class="panel password-change-panel">
            <div class="session-panel-head">
                <div>
                    <p class="section-label">"Credential"</p>
                    <h2>"Change password"</h2>
                </div>
            </div>
            <p class="passkey-lede">
                "Enter your current password to confirm it's you. Use at least 15 characters for the new password. Other signed-in sessions will be signed out."
            </p>
            <div class="auth-fields">
                <label class="auth-field">
                    <span>"Current password"</span>
                    <input
                        class="auth-input"
                        type="password"
                        autocomplete="current-password"
                        prop:value=move || current_password.get()
                        on:input=move |event| {
                            set_client_error.set(None);
                            set_current_password.set(event_target_value(&event));
                        }
                    />
                </label>
                <label class="auth-field">
                    <span>"New password"</span>
                    <input
                        class="auth-input"
                        type="password"
                        autocomplete="new-password"
                        prop:value=move || new_password.get()
                        on:input=move |event| {
                            set_client_error.set(None);
                            set_new_password.set(event_target_value(&event));
                        }
                    />
                    <small>"Minimum 15 characters. Prefer a long phrase."</small>
                </label>
                <label class="auth-field">
                    <span>"Confirm new password"</span>
                    <input
                        class="auth-input"
                        type="password"
                        autocomplete="new-password"
                        prop:value=move || confirm_password.get()
                        on:input=move |event| {
                            set_client_error.set(None);
                            set_confirm_password.set(event_target_value(&event));
                        }
                    />
                </label>
            </div>
            <div class="account-card-actions">
                <button
                    type="button"
                    class="primary-button"
                    disabled=move || !can_submit()
                    on:click=move |_| {
                        let current = current_password.get_untracked();
                        let next = new_password.get_untracked();
                        let confirm = confirm_password.get_untracked();
                        if next.chars().count() < 15 {
                            set_client_error.set(Some("New password must be at least 15 characters.".to_owned()));
                            return;
                        }
                        if next != confirm {
                            set_client_error.set(Some("New password and confirmation do not match.".to_owned()));
                            return;
                        }
                        if next == current {
                            set_client_error.set(Some("New password must be different from the current password.".to_owned()));
                            return;
                        }
                        set_client_error.set(None);
                        action.dispatch(ChangePassword {
                            current_password: current,
                            new_password: next,
                        });
                    }
                >
                    {move || if pending.get() { "Updating password…" } else { "Update password" }}
                </button>
                <p class="error-banner" hidden=move || client_error.get().is_none()>
                    {move || client_error.get().unwrap_or_default()}
                </p>
                <Show when=move || value.get().is_some()>
                    <p class="result-line">{move || action_result_text(value.get())}</p>
                </Show>
                <Show when=move || matches!(value.get(), Some(Ok(_)))>
                    <p class="auth-success">
                        <span>"Password updated. Other sessions were signed out."</span>
                    </p>
                </Show>
                <a class="auth-text-link" href="/forgot-password">"Forgot password? Use email reset"</a>
            </div>
        </section>
    }
}

#[island(lazy)]
fn AccountSessionManager() -> impl IntoView {
    let sessions = browser_load(list_account_sessions);
    let revoke_action = ServerAction::<RevokeAccountSession>::new();
    let revoke_pending = revoke_action.pending();
    let revoke_value = revoke_action.value();
    let (rows, set_rows) = signal(Vec::<AccountSessionSummary>::new());
    let (pending_id, set_pending_id) = signal(None::<String>);
    let (pending_is_current, set_pending_is_current) = signal(false);
    let (status_message, set_status_message) = signal(None::<String>);
    let (error_message, set_error_message) = signal(None::<String>);
    let (signing_out, set_signing_out) = signal(false);

    Effect::new(move |_| {
        if let Some(Ok(response)) = sessions.get() {
            set_rows.set(response.sessions);
        }
    });

    Effect::new(move |_| match revoke_value.get() {
        Some(Ok(_)) => {
            let id = pending_id.get_untracked();
            let was_current = pending_is_current.get_untracked();
            set_pending_id.set(None);
            set_error_message.set(None);
            if was_current {
                // Self-revoke: cookie cleared server-side — leave immediately (hard nav).
                set_signing_out.set(true);
                set_status_message.set(Some("Signing you out…".to_owned()));
                redirect_browser("/login");
                #[cfg(feature = "hydrate")]
                if let Some(window) = window() {
                    let _ = window.location().set_href("/login");
                }
                return;
            }
            if let Some(id) = id {
                set_rows.update(|list| list.retain(|session| session.session_id != id));
            }
            set_status_message.set(Some(
                "Session revoked. That device is signed out immediately if online, or on its next request if offline."
                    .to_owned(),
            ));
        }
        Some(Err(error)) => {
            set_pending_id.set(None);
            set_pending_is_current.set(false);
            set_signing_out.set(false);
            set_status_message.set(None);
            set_error_message.set(Some(server_error_text(error)));
        }
        None => {}
    });

    view! {
        <section class="panel">
            <div class="session-panel-head">
                <div>
                    <p class="section-label">"Devices"</p>
                    <h2>"Active sessions"</h2>
                </div>
            </div>
            <p class="passkey-lede">
                "Revoking ends access for that browser or device. Signing out this browser leaves the page immediately."
            </p>
            <div class="client-data-slot">
                {move || match sessions.get() {
                    Some(Ok(_)) => {
                        let list = rows.get();
                        if list.is_empty() {
                            view! { <p class="result-line">"No active sessions"</p> }.into_any()
                        } else {
                            view! {
                                <div class="session-list">
                                    <For
                                        each=move || rows.get()
                                        key=|session| session.session_id.clone()
                                        children=move |session| {
                                            let session_id = session.session_id.clone();
                                            let session_id_disabled = session_id.clone();
                                            let session_id_click = session_id.clone();
                                            let session_id_label = session_id.clone();
                                            let is_current = session.current;
                                            let assurance = session.assurance.clone();
                                            let expires = session.expires_at_ms;
                                            view! {
                                                <article class=if is_current {
                                                    "compact-panel session-card session-card-current"
                                                } else {
                                                    "compact-panel session-card"
                                                }>
                                                    <div class="session-card-head">
                                                        <h3>{if is_current { "This browser" } else { "Other device" }}</h3>
                                                        <span class="session-assurance">{assurance.to_uppercase()}</span>
                                                    </div>
                                                    <p class="result-line">
                                                        {format!("Expires at {expires}")}
                                                    </p>
                                                    <button
                                                        type="button"
                                                        class=if is_current { "primary-button" } else { "secondary-button" }
                                                        disabled=move || {
                                                            revoke_pending.get()
                                                                || signing_out.get()
                                                                || pending_id.get().as_deref()
                                                                    == Some(session_id_disabled.as_str())
                                                        }
                                                        on:click=move |_| {
                                                            set_error_message.set(None);
                                                            set_status_message.set(None);
                                                            set_pending_id.set(Some(session_id_click.clone()));
                                                            set_pending_is_current.set(is_current);
                                                            if is_current {
                                                                set_signing_out.set(true);
                                                                set_status_message.set(Some(
                                                                    "Signing you out of this browser…".to_owned(),
                                                                ));
                                                            }
                                                            revoke_action.dispatch(RevokeAccountSession {
                                                                session_id: session_id_click.clone(),
                                                            });
                                                        }
                                                    >
                                                        {move || {
                                                            let this_pending = pending_id.get().as_deref()
                                                                == Some(session_id_label.as_str())
                                                                && (revoke_pending.get() || signing_out.get());
                                                            if this_pending {
                                                                if is_current { "Signing out…" } else { "Revoking…" }
                                                            } else if is_current {
                                                                "Sign out this browser"
                                                            } else {
                                                                "Revoke access"
                                                            }
                                                        }}
                                                    </button>
                                                </article>
                                            }
                                        }
                                    />
                                </div>
                            }.into_any()
                        }
                    }
                    Some(Err(error)) => view! { <p class="error-banner">{server_error_text(error)}</p> }.into_any(),
                    None => view! { <p class="result-line">"Loading sessions"</p> }.into_any(),
                }}
            </div>
            <p class="auth-success" hidden=move || status_message.get().is_none() || error_message.get().is_some()>
                {move || status_message.get().unwrap_or_default()}
            </p>
            <p class="error-banner" hidden=move || error_message.get().is_none()>
                {move || error_message.get().unwrap_or_default()}
            </p>
        </section>
    }
}

#[island(lazy)]
fn OrganizationsPage() -> impl IntoView {
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

fn org_monogram(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect();
    let mut chars = cleaned.chars();
    match (chars.next(), chars.next()) {
        (Some(a), Some(b)) => format!(
            "{}{}",
            a.to_ascii_uppercase(),
            b.to_ascii_uppercase()
        ),
        (Some(a), None) => a.to_ascii_uppercase().to_string(),
        _ => "?".to_owned(),
    }
}

fn org_tone_index(name: &str) -> u8 {
    let hash = name.bytes().fold(0u32, |acc, b| acc.wrapping_mul(33).wrapping_add(b as u32));
    (hash % 6) as u8
}

#[component]
fn OrganizationSettingsPage() -> impl IntoView {
    page_shell(
        "Organization settings",
        "The selected tenant comes from the verified session, never from an untrusted form alone.",
        view! { <SessionSummary /> <OrganizationLinks /> },
    )
}

#[island(lazy)]
fn OrganizationMembersPage() -> impl IntoView {
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
fn OrganizationInvitationsPage() -> impl IntoView {
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
fn OrganizationRolesPage() -> impl IntoView {
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
fn OrganizationPermissionsPage() -> impl IntoView {
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
fn OrganizationAuditPage() -> impl IntoView {
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
fn AdminUsersPage() -> impl IntoView {
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
fn AdminHealthPage() -> impl IntoView {
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
fn AdminPoliciesPage() -> impl IntoView {
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
fn NotFoundPage() -> impl IntoView {
    set_page_status(http::StatusCode::NOT_FOUND);
    error_page_shell(
        "Not found",
        "This page does not exist.",
        view! { <ReturnToLoginLink /> },
    )
}

#[island]
fn EmailPasswordAuthForm(register_default: bool) -> impl IntoView {
    let login_action = ServerAction::<LoginEmailPassword>::new();
    let register_action = ServerAction::<RegisterEmailPassword>::new();
    let login_pending = login_action.pending();
    let register_pending = register_action.pending();
    let login_value = login_action.value();
    let register_value = register_action.value();
    let (register_mode, set_register_mode) = signal(register_default);
    // Apple-style sign-in: email → Continue → password + Sign in.
    let (password_step, set_password_step) = signal(register_default);
    let (email, set_email) = signal(String::new());
    let (password, set_password) = signal(String::new());
    let (client_error, set_client_error) = signal(None::<String>);
    let capture_enabled = browser_load(development_mail_capture_enabled);
    let capture_action = ServerAction::<LatestDevelopmentMail>::new();
    let capture_pending = capture_action.pending();
    let capture_value = capture_action.value();
    let registration_complete = RwSignal::new(false);

    // Shared-email passkey (modal + conditional autofill on password step).
    let passkey_start = ServerAction::<StartPasskeyLogin>::new();
    let passkey_verify = ServerAction::<VerifyPasskeyLogin>::new();
    let passkey_start_pending = passkey_start.pending();
    let passkey_verify_pending = passkey_verify.pending();
    let passkey_start_value = passkey_start.value();
    let passkey_verify_value = passkey_verify.value();
    let (passkey_mediation, set_passkey_mediation) = signal("".to_string());
    let (conditional_armed, set_conditional_armed) = signal(false);
    let capabilities = browser_load(get_auth_capabilities);

    Effect::new(move |_| {
        if let Some(Ok(response)) = login_value.get() {
            redirect_browser(&response.redirect_url);
        }
    });
    Effect::new(move |_| {
        if register_value.get().is_some_and(|result| result.is_ok()) {
            registration_complete.set(true);
        }
    });
    Effect::new(move |_| {
        if let Some(Ok(message)) = capture_value.get() {
            if let Some(action_url) = message.action_url.as_deref().filter(|url| !url.is_empty()) {
                redirect_browser(action_url);
            } else if let Some(action_url) = first_http_url_in_text(&message.body_text) {
                redirect_browser(&action_url);
            }
        }
    });
    Effect::new(move |_| {
        match passkey_verify_value.get() {
            Some(Ok(response)) => redirect_browser(&response.redirect_url),
            // Single error channel: client_error only (avoid a second auth-error block).
            Some(Err(error)) if passkey_mediation.get_untracked() != "conditional" => {
                set_client_error.set(Some(server_error_text(error)));
            }
            _ => {}
        }
    });
    Effect::new(move |_| {
        let mediation = passkey_mediation.get_untracked();
        let silent = mediation == "conditional";
        match passkey_start_value.get() {
            Some(Ok(response)) => {
                #[cfg(feature = "hydrate")]
                {
                    if !passkey_supported() {
                        if !silent {
                            redirect_browser("/auth/passkey-unsupported");
                        }
                        return;
                    }
                    let verify = passkey_verify;
                    let set_client_error = set_client_error;
                    let challenge_id = response.challenge_id;
                    let options_json = response.public_key_options_json;
                    let redirect_url = Some(next_url());
                    spawn_local(async move {
                        match get_passkey_credential(options_json, mediation.clone()).await {
                            Ok(value) => match passkey_js_string(value) {
                                Ok(credential_json) => {
                                    verify.dispatch(VerifyPasskeyLogin {
                                        challenge_id,
                                        credential_json,
                                        redirect_url,
                                    });
                                }
                                Err(error) => {
                                    if !silent {
                                        set_client_error.set(Some(error));
                                    }
                                }
                            },
                            Err(error) => {
                                let message = passkey_js_error(error);
                                if silent
                                    || message.contains("PASSKEY_CONDITIONAL_IDLE")
                                    || message.contains("cancelled")
                                {
                                    return;
                                }
                                set_client_error.set(Some(message));
                            }
                        }
                    });
                }
                #[cfg(not(feature = "hydrate"))]
                {
                    let _ = (response, mediation, silent);
                }
            }
            // Conditional start failures must never surface as login form errors.
            Some(Err(_)) if silent => {}
            // Modal passkey start failed (e.g. no passkey for this email) — one banner only.
            Some(Err(error)) => {
                let text = server_error_text(error);
                // Server often uses a generic credential error for privacy; be clearer on passkey click.
                let text = if text.to_ascii_lowercase().contains("incorrect")
                    || text.to_ascii_lowercase().contains("invalid credential")
                    || text.to_ascii_lowercase().contains("not found")
                {
                    "No passkey is available for this email. Sign in with your password, then add a passkey in Account → Passkeys.".to_owned()
                } else {
                    text
                };
                set_client_error.set(Some(text));
            }
            None => {}
        }
    });

    // After email is confirmed, arm Conditional UI so Chrome/Safari can offer a passkey in autofill.
    // Failures are silent — accounts without passkeys are normal.
    Effect::new(move |_| {
        let passkeys_on = capabilities
            .get()
            .and_then(Result::ok)
            .is_some_and(|caps| caps.passkeys_enabled);
        let ready = !register_mode.get()
            && password_step.get()
            && passkeys_on
            && !conditional_armed.get()
            && validate_email_only(&email.get()).is_ok();
        if !ready {
            return;
        }
        set_conditional_armed.set(true);
        #[cfg(feature = "hydrate")]
        {
            let email_value = email.get_untracked().trim().to_string();
            let passkey_start = passkey_start;
            let set_passkey_mediation = set_passkey_mediation;
            spawn_local(async move {
                let available = is_conditional_mediation_available()
                    .await
                    .ok()
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false);
                if !available {
                    return;
                }
                set_passkey_mediation.set("conditional".to_owned());
                passkey_start.dispatch(StartPasskeyLogin {
                    email: Some(email_value),
                    redirect_url: Some(next_url()),
                });
            });
        }
    });

    let submit_credentials = move || {
        // Sign-in step 1: email only → reveal password + Sign in.
        if !register_mode.get_untracked() && !password_step.get_untracked() {
            let email_value = email.get_untracked().trim().to_string();
            if let Err(error) = validate_email_only(&email_value) {
                set_client_error.set(Some(error));
                return;
            }
            set_email.set(email_value);
            set_client_error.set(None);
            set_password_step.set(true);
            set_conditional_armed.set(false);
            return;
        }
        let email_value = email.get_untracked().trim().to_string();
        let password_value = password.get_untracked();
        if let Err(error) =
            validate_login_form(&email_value, &password_value, register_mode.get_untracked())
        {
            set_client_error.set(Some(error));
            return;
        }
        set_client_error.set(None);
        let redirect_url = Some(next_url());
        if register_mode.get_untracked() {
            register_action.dispatch(RegisterEmailPassword {
                email: email_value,
                password: password_value,
                redirect_url,
            });
        } else {
            login_action.dispatch(LoginEmailPassword {
                email: email_value,
                password: password_value,
                redirect_url,
            });
        }
    };

    let start_passkey_modal = move |_| {
        let email_value = email.get_untracked().trim().to_string();
        if let Err(error) = validate_email_only(&email_value) {
            set_client_error.set(Some(error));
            // Stay on email step so the shared field is obvious.
            set_password_step.set(false);
            return;
        }
        set_client_error.set(None);
        // Modal ceremony — surface errors if the user explicitly chose passkey.
        // Password is ignored for passkey sign-in; only the shared email is used.
        set_passkey_mediation.set(String::new());
        set_conditional_armed.set(true); // avoid a racing conditional start
        passkey_start.dispatch(StartPasskeyLogin {
            email: Some(email_value),
            redirect_url: Some(next_url()),
        });
    };

    view! {
        <section class="auth-form">
            <div>
                <p class="auth-kicker">"Authentication"</p>
                <h1 class="auth-title">
                    {move || if register_mode.get() { "Create your workspace" } else { "Welcome back" }}
                </h1>
                <p class="auth-copy">
                    {move || if register_mode.get() {
                        "Set up a password-backed workspace session."
                    } else if password_step.get() {
                        "Enter your password to continue."
                    } else {
                        "Enter your email to continue."
                    }}
                </p>
            </div>

            <Show when=move || registration_complete.get()>
                <div class="auth-success">
                    <p><strong>"Account created."</strong> " Check your inbox for the one-time verification link."</p>
                    <Show when=move || matches!(capture_enabled.get(), Some(Ok(true)))>
                        <p>"Capture mode does not send internet email. The local worker stores the message for this example."</p>
                        <button
                            type="button"
                            class="auth-secondary"
                            disabled=move || capture_pending.get()
                            on:click=move |_| {
                                capture_action.dispatch(LatestDevelopmentMail {
                                    recipient: email.get_untracked(),
                                    message_kind: "email-verification".to_owned(),
                                });
                            }
                        >
                            {move || if capture_pending.get() { "Looking for message" } else { "Open captured verification link" }}
                        </button>
                        <Show when=move || selected_action_error(capture_value.get()).is_some()>
                            <p class="auth-inline-error">
                                {move || selected_action_error(capture_value.get()).unwrap_or_default()}
                            </p>
                        </Show>
                    </Show>
                </div>
            </Show>

            <div class="auth-mode-switch" role="tablist" aria-label="Authentication mode" hidden=move || registration_complete.get()>
                <button
                    type="button"
                    class=move || if register_mode.get() {
                        "auth-mode-button"
                    } else {
                        "auth-mode-button auth-mode-button-active"
                    }
                    on:click=move |_| {
                        set_register_mode.set(false);
                        set_password_step.set(false);
                        set_password.set(String::new());
                        set_conditional_armed.set(false);
                        set_client_error.set(None);
                    }
                >
                    "Sign in"
                </button>
                <button
                    type="button"
                    class=move || if register_mode.get() {
                        "auth-mode-button auth-mode-button-active"
                    } else {
                        "auth-mode-button"
                    }
                    on:click=move |_| {
                        set_register_mode.set(true);
                        set_password_step.set(true);
                        set_conditional_armed.set(false);
                        set_client_error.set(None);
                    }
                >
                    "Create workspace"
                </button>
            </div>

            <form class="auth-fields" hidden=move || registration_complete.get() on:submit=move |event| {
                event.prevent_default();
                submit_credentials();
            }>
                <label class="auth-field">
                    <span>"Email"</span>
                    <input
                        class="auth-input"
                        type="email"
                        name="email"
                        // webauthn enables Conditional UI passkey rows in supporting browsers
                        autocomplete="username webauthn"
                        placeholder="name@company.com"
                        prop:value=move || email.get()
                        aria-invalid=move || client_error.get().is_some()
                        on:input=move |event| {
                            set_email.set(event_target_value(&event));
                            set_client_error.set(None);
                            // Changing email invalidates any armed conditional ceremony.
                            set_conditional_armed.set(false);
                        }
                    />
                </label>
                <label
                    class="auth-field"
                    hidden=move || !register_mode.get() && !password_step.get()
                >
                    <span>"Password"</span>
                    <input
                        class="auth-input"
                        type="password"
                        name="password"
                        autocomplete=move || if register_mode.get() { "new-password" } else { "current-password" }
                        placeholder="Enter your password"
                        prop:value=move || password.get()
                        aria-invalid=move || client_error.get().is_some()
                        on:input=move |event| {
                            set_password.set(event_target_value(&event));
                            set_client_error.set(None);
                        }
                    />
                    <small hidden=move || !register_mode.get()>
                        "Use 15 to 128 characters. Only a derived password hash is stored."
                    </small>
                </label>

                // One error banner only: client validation + passkey + password/register server errors.
                <p
                    class="auth-error"
                    hidden=move || {
                        client_error.get().is_none()
                            && selected_auth_error(
                                register_mode.get(),
                                login_value.get(),
                                register_value.get(),
                            )
                            .is_none()
                    }
                >
                    {move || {
                        client_error
                            .get()
                            .or_else(|| {
                                selected_auth_error(
                                    register_mode.get(),
                                    login_value.get(),
                                    register_value.get(),
                                )
                            })
                            .unwrap_or_default()
                    }}
                </p>

                <button
                    type="submit"
                    class="auth-submit"
                    disabled=move || {
                        login_pending.get()
                            || register_pending.get()
                            || passkey_start_pending.get()
                            || passkey_verify_pending.get()
                    }
                    aria-busy=move || {
                        if login_pending.get() || register_pending.get() {
                            "true"
                        } else {
                            "false"
                        }
                    }
                >
                    <span
                        class="auth-button-spinner"
                        aria-hidden="true"
                        hidden=move || !(login_pending.get() || register_pending.get())
                    ></span>
                    <span>
                        {move || if login_pending.get() || register_pending.get() {
                            if register_mode.get() { "Creating workspace" } else { "Signing in" }
                        } else if register_mode.get() {
                            "Create workspace"
                        } else if password_step.get() {
                            "Sign in"
                        } else {
                            "Continue"
                        }}
                    </span>
                </button>
                <a
                    class="auth-text-link"
                    href="/forgot-password"
                    hidden=move || register_mode.get() || !password_step.get()
                >
                    "Forgot password?"
                </a>
                <button
                    type="button"
                    class="auth-text-link auth-text-button"
                    hidden=move || register_mode.get() || !password_step.get()
                    on:click=move |_| {
                        set_password_step.set(false);
                        set_password.set(String::new());
                        set_conditional_armed.set(false);
                        set_client_error.set(None);
                    }
                >
                    "Use a different email"
                </button>
            </form>

            <div
                class="auth-alt-methods"
                hidden=move || registration_complete.get()
                    || !capabilities.get().is_some_and(|result| {
                        result.is_ok_and(|caps| {
                            (caps.oauth_enabled && !caps.providers.is_empty())
                                || caps.passkeys_enabled
                        })
                    })
            >
                <div class="auth-divider" aria-hidden="true">
                    <span>"or"</span>
                </div>
                <div
                    class="auth-alt-stack"
                    hidden=move || !capabilities.get().is_some_and(|result| {
                        result.is_ok_and(|caps| caps.oauth_enabled && !caps.providers.is_empty())
                    })
                >
                    <OAuthProviderButtons />
                </div>
                <button
                    type="button"
                    class="auth-alt-button"
                    hidden=move || !capabilities.get().is_some_and(|result| {
                        result.is_ok_and(|caps| caps.passkeys_enabled)
                    })
                    disabled=move || {
                        passkey_start_pending.get()
                            || passkey_verify_pending.get()
                            || login_pending.get()
                            || register_pending.get()
                    }
                    on:click=start_passkey_modal
                >
                    {move || if passkey_start_pending.get() || passkey_verify_pending.get() {
                        "Waiting for passkey…"
                    } else {
                        "Sign in with Passkey"
                    }}
                </button>
            </div>

            <p class="auth-trust-copy">
                "Protected by server-side validation, httpOnly session cookies, and tenant-scoped authorization checks."
            </p>
        </section>
    }
}

#[island]
fn EmailVerificationForm() -> impl IntoView {
    let action = ServerAction::<CompleteEmailVerification>::new();
    let value = action.value();
    let pending = action.pending();
    let dispatched = RwSignal::new(false);

    Effect::new(move |_| {
        if !dispatched.get()
            && let Some(token) = one_time_token_from_url()
        {
            dispatched.set(true);
            action.dispatch(CompleteEmailVerification {
                token,
                redirect_url: Some("/dashboard".to_string()),
            });
        }
    });
    Effect::new(move |_| {
        if let Some(Ok(response)) = value.get() {
            redirect_browser(&response.redirect_url);
        }
    });

    view! {
        <section class="auth-form">
            <div>
                <p class="auth-kicker">"Email verification"</p>
                <h1 class="auth-title">"Verify your email"</h1>
                <p class="auth-copy">"The one-time link is hashed at rest and can be used once."</p>
            </div>
            <Show when=move || pending.get()>
                <p class="result-line">"Verifying email"</p>
            </Show>
            <Show when=move || selected_action_error(value.get()).is_some()>
                <p class="auth-error">{move || selected_action_error(value.get()).unwrap_or_default()}</p>
            </Show>
            <Show when=move || one_time_token_from_url().is_none()>
                <p class="auth-notice">"Open this page from the one-time link in your verification message."</p>
            </Show>
            <a class="auth-text-link" href="/verify-email/resend">"Request another message"</a>
        </section>
    }
}

#[island]
fn ResendVerificationForm() -> impl IntoView {
    let action = ServerAction::<ResendEmailVerification>::new();
    let pending = action.pending();
    let value = action.value();
    let (email, set_email) = signal(String::new());
    let capture_enabled = browser_load(development_mail_capture_enabled);
    let capture_action = ServerAction::<LatestDevelopmentMail>::new();
    let capture_pending = capture_action.pending();
    let capture_value = capture_action.value();

    Effect::new(move |_| {
        if let Some(Ok(message)) = capture_value.get() {
            if let Some(action_url) = message.action_url.as_deref().filter(|url| !url.is_empty()) {
                redirect_browser(action_url);
            } else if let Some(action_url) = first_http_url_in_text(&message.body_text) {
                redirect_browser(&action_url);
            }
        }
    });

    view! {
        <section class="auth-form">
            <div>
                <p class="auth-kicker">"Email verification"</p>
                <h1 class="auth-title">"Send a fresh link"</h1>
                <p class="auth-copy">"The response is generic whether or not the account exists."</p>
            </div>
            <form class="auth-fields" on:submit=move |event| {
                event.prevent_default();
                action.dispatch(ResendEmailVerification {
                    email: email.get_untracked(),
                    redirect_url: Some("/dashboard".to_string()),
                });
            }>
                <label class="auth-field">
                    <span>"Email"</span>
                    <input
                        class="auth-input"
                        type="email"
                        autocomplete="email"
                        prop:value=move || email.get()
                        on:input=move |event| set_email.set(event_target_value(&event))
                    />
                </label>
                <button type="submit" class="auth-submit" disabled=move || pending.get()>
                    "Send verification link"
                </button>
                <Show when=move || value.get().is_some()>
                    <p class="result-line">{move || action_result_text(value.get())}</p>
                </Show>
                <Show when=move || value.get().is_some() && matches!(capture_enabled.get(), Some(Ok(true)))>
                    <p class="auth-notice">"Capture mode stores this message locally; it will not arrive in an external inbox."</p>
                    <button
                        type="button"
                        class="auth-secondary"
                        disabled=move || capture_pending.get()
                        on:click=move |_| {
                            capture_action.dispatch(LatestDevelopmentMail {
                                recipient: email.get_untracked(),
                                message_kind: "email-verification".to_owned(),
                            });
                        }
                    >
                        {move || if capture_pending.get() { "Looking for message" } else { "Open captured verification link" }}
                    </button>
                    <Show when=move || selected_action_error(capture_value.get()).is_some()>
                        <p class="auth-inline-error">{move || selected_action_error(capture_value.get()).unwrap_or_default()}</p>
                    </Show>
                </Show>
            </form>
        </section>
    }
}

#[island]
fn ForgotPasswordForm() -> impl IntoView {
    let action = ServerAction::<StartPasswordReset>::new();
    let pending = action.pending();
    let value = action.value();
    let (email, set_email) = signal(String::new());
    let (client_error, set_client_error) = signal(None::<String>);

    let submit = move || {
        let email_value = email.get_untracked().trim().to_string();
        if let Err(error) = validate_email_only(&email_value) {
            set_client_error.set(Some(error));
            return;
        }
        set_client_error.set(None);
        action.dispatch(StartPasswordReset {
            email: email_value,
            redirect_url: Some("/dashboard".to_string()),
        });
    };

    view! {
        <section class="auth-form">
            <div>
                <p class="auth-kicker">"Password reset"</p>
                <h1 class="auth-title">"Recover access"</h1>
                <p class="auth-copy">
                    "Enter your email and we will send reset instructions if an account exists."
                </p>
            </div>
            <form class="auth-fields" on:submit=move |event| {
                event.prevent_default();
                submit();
            }>
                <label class="auth-field">
                    <span>"Email"</span>
                    <input
                        class="auth-input"
                        type="email"
                        name="email"
                        autocomplete="username"
                        placeholder="name@company.com"
                        prop:value=move || email.get()
                        aria-invalid=move || client_error.get().is_some()
                        on:input=move |event| {
                            set_email.set(event_target_value(&event));
                            set_client_error.set(None);
                        }
                    />
                    <small>"For privacy, the response is the same even if no account exists."</small>
                </label>
                <p
                    class="auth-error"
                    hidden=move || client_error.get().is_none()
                >
                    {move || client_error.get().unwrap_or_default()}
                </p>
                <div hidden=move || value.get().is_none()>
                    <PasswordResetStartResult result=move || value.get() />
                </div>
                <button
                    type="submit"
                    class="auth-submit"
                    disabled=move || pending.get()
                    aria-busy=move || if pending.get() { "true" } else { "false" }
                >
                    <span
                        class="auth-button-spinner"
                        aria-hidden="true"
                        hidden=move || !pending.get()
                    ></span>
                    <span>{move || if pending.get() { "Sending reset link" } else { "Send reset link" }}</span>
                </button>
                <a class="auth-text-link" href="/login">"Return to sign in"</a>
            </form>
        </section>
    }
}

#[component]
fn PasswordResetStartResult(
    result: impl Fn() -> Option<Result<PasswordResetStartResponse, ServerFnError>>
    + Copy
    + Send
    + 'static,
) -> impl IntoView {
    view! {
        {move || match result() {
            Some(Ok(response)) => {
                let _ = response;
                view! {
                    <div class="auth-success">
                        <p>"If an account exists, reset instructions are ready to send."</p>
                    </div>
                }.into_any()
            }
            Some(Err(error)) => view! { <p class="auth-error">{server_error_text(error)}</p> }.into_any(),
            None => view! {}.into_any(),
        }}
    }
}

#[island]
fn ResetPasswordForm() -> impl IntoView {
    let action = ServerAction::<CompletePasswordReset>::new();
    let pending = action.pending();
    let value = action.value();
    let (password, set_password) = signal(String::new());
    let (client_error, set_client_error) = signal(None::<String>);

    Effect::new(move |_| {
        if let Some(Ok(response)) = value.get() {
            redirect_browser(&response.redirect_url);
        }
    });

    let submit = move || {
        let token = one_time_token_from_url();
        let password_value = password.get_untracked();
        if token.is_none() {
            set_client_error.set(Some("Reset token is missing.".to_string()));
            return;
        }
        if !(15..=128).contains(&password_value.chars().count()) {
            set_client_error.set(Some(
                "Password must contain 15 to 128 characters.".to_string(),
            ));
            return;
        }
        set_client_error.set(None);
        action.dispatch(CompletePasswordReset {
            token: token.unwrap_or_default(),
            password: password_value,
            redirect_url: Some("/dashboard".to_string()),
        });
    };

    view! {
        <section class="auth-form">
            <div>
                <p class="auth-kicker">"Password reset"</p>
                <h1 class="auth-title">"Choose a new password"</h1>
                <p class="auth-copy">
                    "Use the reset link once. After the password changes, a new session is issued."
                </p>
            </div>
            <form class="auth-fields" on:submit=move |event| {
                event.prevent_default();
                submit();
            }>
                <label class="auth-field">
                    <span>"New password"</span>
                    <input
                        class="auth-input"
                        type="password"
                        name="password"
                        autocomplete="new-password"
                        placeholder="Enter your new password"
                        prop:value=move || password.get()
                        aria-invalid=move || client_error.get().is_some()
                        on:input=move |event| {
                            set_password.set(event_target_value(&event));
                            set_client_error.set(None);
                        }
                    />
                    <small>"Use 15 to 128 characters. Existing sessions should be reviewed after reset."</small>
                </label>
                <p
                    class="auth-error"
                    hidden=move || client_error.get().is_none()
                >
                    {move || client_error.get().unwrap_or_default()}
                </p>
                <p
                    class="auth-error"
                    hidden=move || selected_action_error(value.get()).is_none()
                >
                    {move || selected_action_error(value.get()).unwrap_or_default()}
                </p>
                <button
                    type="submit"
                    class="auth-submit"
                    disabled=move || pending.get()
                    aria-busy=move || if pending.get() { "true" } else { "false" }
                >
                    <span
                        class="auth-button-spinner"
                        aria-hidden="true"
                        hidden=move || !pending.get()
                    ></span>
                    <span>{move || if pending.get() { "Updating password" } else { "Reset password" }}</span>
                </button>
                <a class="auth-text-link" href="/login">"Return to sign in"</a>
            </form>
        </section>
    }
}

#[island]
fn InvitationAcceptForm() -> impl IntoView {
    let action = ServerAction::<AcceptOrganizationInvitation>::new();
    let pending = action.pending();
    let value = action.value();
    let (client_error, set_client_error) = signal(None::<String>);
    let (accepted_org, set_accepted_org) = signal(None::<OrganizationSummary>);

    Effect::new(move |_| {
        if let Some(Ok(organization)) = value.get() {
            set_accepted_org.set(Some(organization));
        }
    });

    let submit = move || {
        let Some(token) = one_time_token_from_url() else {
            set_client_error.set(Some(
                "Invitation token is missing. Open the one-time link from your email.".to_string(),
            ));
            return;
        };
        set_client_error.set(None);
        action.dispatch(AcceptOrganizationInvitation { token });
    };

    view! {
        <section class="auth-form">
            <div>
                <p class="auth-kicker">"Organization invite"</p>
                <h1 class="auth-title">"Accept invitation"</h1>
                <p class="auth-copy">
                    "Join the organization with the account you are signed in as. The invite email must match this account."
                </p>
            </div>
            <Show
                when=move || accepted_org.get().is_some()
                fallback=move || view! {
                    <div class="auth-fields">
                        <p
                            class="auth-error"
                            hidden=move || client_error.get().is_none()
                        >
                            {move || client_error.get().unwrap_or_default()}
                        </p>
                        <p
                            class="auth-error"
                            hidden=move || selected_action_error(value.get()).is_none()
                        >
                            {move || selected_action_error(value.get()).unwrap_or_default()}
                        </p>
                        <Show when=move || one_time_token_from_url().is_none()>
                            <p class="auth-error">
                                "Open this page from the invitation email so the one-time token is present."
                            </p>
                        </Show>
                        <button
                            type="button"
                            class="auth-submit"
                            disabled=move || pending.get() || one_time_token_from_url().is_none()
                            aria-busy=move || if pending.get() { "true" } else { "false" }
                            on:click=move |_| submit()
                        >
                            <span
                                class="auth-button-spinner"
                                aria-hidden="true"
                                hidden=move || !pending.get()
                            ></span>
                            <span>
                                {move || {
                                    if pending.get() {
                                        "Accepting invitation"
                                    } else {
                                        "Accept invitation"
                                    }
                                }}
                            </span>
                        </button>
                        <a class="auth-text-link" href="/organizations">"Back to organizations"</a>
                    </div>
                }
            >
                <div class="auth-success">
                    <p>
                        {move || {
                            accepted_org
                                .get()
                                .map(|org| {
                                    format!(
                                        "You joined {}. Role: {}.",
                                        org.name, org.current_user_role
                                    )
                                })
                                .unwrap_or_default()
                        }}
                    </p>
                    <div class="actions">
                        <a class="link-button link-button-primary" href="/organizations">
                            "Open organizations"
                        </a>
                        <a class="link-button" href="/dashboard">"Dashboard"</a>
                    </div>
                </div>
            </Show>
        </section>
    }
}

/// Flat OAuth buttons for the login card (Apple-style alternative methods).
#[component]
fn OAuthProviderButtons() -> impl IntoView {
    let providers = browser_load(list_auth_providers);

    view! {
        <div class="auth-alt-stack">
            <For
                each=move || match providers.get() {
                    Some(Ok(providers)) => providers,
                    _ => Vec::new(),
                }
                key=|provider| provider.provider_id.clone()
                children=move |provider| view! {
                    <ProviderLoginButton
                        provider_id=provider.provider_id
                        label=provider.display_name
                    />
                }
            />
        </div>
    }
}

#[component]
fn OAuthProviderList() -> impl IntoView {
    view! {
        <section class="panel compact-panel">
            <h2>"Sign in with a provider"</h2>
            <OAuthProviderButtons />
        </section>
    }
}

#[component]
fn ProviderLoginButton(provider_id: String, label: String) -> impl IntoView {
    let action = ServerAction::<StartOauthLogin>::new();
    let pending = action.pending();
    let value = action.value();
    let provider_for_submit = provider_id.clone();
    let label_for_view = label.clone();

    Effect::new(move |_| {
        if let Some(Ok(response)) = value.get() {
            redirect_browser(&response.authorization_url);
        }
    });

    let submit = move |_| {
        action.dispatch(StartOauthLogin {
            provider_id: provider_for_submit.clone(),
            redirect_url: Some(next_url()),
        });
    };

    view! {
        <button
            type="button"
            class="auth-alt-button"
            disabled=move || pending.get()
            on:click=submit
        >
            {move || if pending.get() {
                format!("Connecting to {label_for_view}…")
            } else {
                format!("Sign in with {label_for_view}")
            }}
        </button>
        <Show when=move || matches!(value.get(), Some(Err(_)))>
            <p class="auth-inline-error">{move || action_result_text(value.get())}</p>
        </Show>
    }
}

/// Account passkeys (GitHub / Google / Apple-style):
/// status → create ceremony focus → success. Never renders blank.
#[island(lazy)]
fn PasskeyManager() -> impl IntoView {
    let capabilities = browser_load(get_auth_capabilities);
    let session = browser_load(get_current_session);
    let start_action = ServerAction::<StartPasskeyRegistration>::new();
    let verify_action = ServerAction::<VerifyPasskeyRegistration>::new();
    let start_pending = start_action.pending();
    let verify_pending = verify_action.pending();
    let start_value = start_action.value();
    let verify_value = verify_action.value();
    let (client_error, set_client_error) = signal(None::<String>);
    #[cfg(feature = "hydrate")]
    let (browser_ok, set_browser_ok) = signal(passkey_supported());
    #[cfg(not(feature = "hydrate"))]
    let (browser_ok, _) = signal(true);

    Effect::new(move |_| {
        if let Some(Ok(response)) = start_value.get() {
            #[cfg(feature = "hydrate")]
            {
                if !passkey_supported() {
                    set_browser_ok.set(false);
                    set_client_error.set(Some(
                        "This browser or device does not support passkeys.".to_owned(),
                    ));
                    return;
                }
                let verify_action = verify_action;
                let set_client_error = set_client_error;
                let challenge_id = response.challenge_id;
                let options_json = response.public_key_options_json;
                set_client_error.set(None);
                spawn_local(async move {
                    match create_passkey_credential(options_json).await {
                        Ok(value) => match passkey_js_string(value) {
                            Ok(credential_json) => {
                                verify_action.dispatch(VerifyPasskeyRegistration {
                                    challenge_id,
                                    credential_json,
                                    redirect_url: Some("/account/passkeys".to_owned()),
                                });
                            }
                            Err(error) => set_client_error.set(Some(error)),
                        },
                        Err(error) => set_client_error.set(Some(passkey_js_error(error))),
                    }
                });
            }
            #[cfg(not(feature = "hydrate"))]
            {
                let _ = response;
            }
        }
    });

    view! {
        <div class="passkey-flow">
            {move || {
                // Stay on ceremony surface for prompt, save, OR error (do not flash back).
                let ceremony_active = start_pending.get()
                    || verify_pending.get()
                    || matches!(start_value.get(), Some(Ok(_)))
                    || client_error.get().is_some()
                    || matches!(start_value.get(), Some(Err(_)))
                    || matches!(verify_value.get(), Some(Err(_)));
                let registered_ok = matches!(verify_value.get(), Some(Ok(_)));
                let ceremony_failed = client_error.get().is_some()
                    || matches!(start_value.get(), Some(Err(_)))
                    || matches!(verify_value.get(), Some(Err(_)));
                let ceremony_cancelled = client_error
                    .get()
                    .as_ref()
                    .is_some_and(|message| is_passkey_cancel_message(message));

                // Exclusive focus while OS/browser passkey sheet is active
                if ceremony_active && !registered_ok {
                    return view! {
                        <div class="passkey-focus-wrap">
                            <section class="panel passkey-ceremony-panel">
                                <div class="passkey-wizard-progress" aria-hidden="true">
                                    <span class="passkey-wizard-step is-done">"1"</span>
                                    <span class="passkey-wizard-line is-done"></span>
                                    <span class="passkey-wizard-step is-active">"2"</span>
                                    <span class="passkey-wizard-line"></span>
                                    <span class="passkey-wizard-step">"3"</span>
                                </div>
                                <p class="section-label">"Creating passkey"</p>
                                <h2>
                                    {move || if ceremony_cancelled {
                                        "Passkey not created"
                                    } else if ceremony_failed {
                                        "Could not create passkey"
                                    } else {
                                        "Confirm with your device"
                                    }}
                                </h2>
                                <p class="passkey-lede">
                                    {move || if ceremony_cancelled {
                                        "You closed the browser prompt. Try again when you're ready, or use a different device / security key."
                                    } else if ceremony_failed {
                                        "You can retry the browser prompt, or go back and try another device / security key."
                                    } else {
                                        "Use Face ID, Touch ID, Windows Hello, a phone QR passkey, or a security key. Keep this tab open until the prompt finishes."
                                    }}
                                </p>
                                <div class="passkey-device-card" aria-hidden="true" hidden=move || ceremony_failed>
                                    <div class="passkey-device-icon">
                                        <span></span>
                                        <span></span>
                                    </div>
                                    <p>"Waiting for authenticator…"</p>
                                </div>
                                <p class="result-line" hidden=move || ceremony_failed>
                                    {move || if verify_pending.get() {
                                        "Saving passkey to your account…"
                                    } else if start_pending.get() {
                                        "Starting secure registration…"
                                    } else {
                                        "Follow the prompt on your device"
                                    }}
                                </p>
                                <p
                                    class="error-banner"
                                    hidden=move || {
                                        match client_error.get() {
                                            None => true,
                                            // Cancel is explained by the lede; avoid a red stack-style banner.
                                            Some(message) if is_passkey_cancel_message(&message) => {
                                                true
                                            }
                                            Some(_) => false,
                                        }
                                    }
                                >
                                    {move || client_error.get().unwrap_or_default()}
                                </p>
                                <p class="error-banner" hidden=move || !matches!(start_value.get(), Some(Err(_)))>
                                    {move || match start_value.get() {
                                        Some(Err(error)) => server_error_text(error),
                                        _ => String::new(),
                                    }}
                                </p>
                                <p class="error-banner" hidden=move || !matches!(verify_value.get(), Some(Err(_)))>
                                    {move || match verify_value.get() {
                                        Some(Err(error)) => server_error_text(error),
                                        _ => String::new(),
                                    }}
                                </p>
                                <div class="button-row">
                                    <button
                                        type="button"
                                        class="primary-button"
                                        disabled=move || start_pending.get() || verify_pending.get()
                                        on:click=move |_| {
                                            set_client_error.set(None);
                                            let email = session
                                                .get_untracked()
                                                .and_then(Result::ok)
                                                .and_then(|s| s.primary_email);
                                            start_action.dispatch(StartPasskeyRegistration {
                                                email,
                                                redirect_url: Some("/account/passkeys".to_owned()),
                                            });
                                        }
                                    >"Try again"</button>
                                    <button
                                        type="button"
                                        class="secondary-button"
                                        on:click=move |_| {
                                            set_client_error.set(None);
                                            redirect_browser("/account/passkeys");
                                        }
                                    >"Back"</button>
                                </div>
                            </section>
                        </div>
                    }.into_any();
                }

                if registered_ok {
                    return view! {
                        <div class="passkey-focus-wrap">
                            <section class="panel passkey-success-panel">
                                <span class="mfa-badge mfa-badge-on">"Passkey registered"</span>
                                <h2>"You can sign in without a password"</h2>
                                <p class="passkey-lede">
                                    "Next time, choose passkey on the sign-in page and approve with this device. Your session assurance is elevated for phishing-resistant sign-in."
                                </p>
                                <div class="button-row">
                                    <a class="primary-button" href="/account/sessions">"Review sessions"</a>
                                    <a class="secondary-button" href="/account/profile">"Back to profile"</a>
                                </div>
                            </section>
                        </div>
                    }.into_any();
                }

                // Default overview (always visible — never blank)
                let session_email = session
                    .get()
                    .and_then(Result::ok)
                    .and_then(|s| s.primary_email)
                    .unwrap_or_else(|| "your account".to_owned());
                let email_for_register = session
                    .get()
                    .and_then(Result::ok)
                    .and_then(|s| s.primary_email);
                let can_register = email_for_register.is_some();
                let email_dispatch = email_for_register.clone();
                let caps = capabilities.get();
                let passkeys_on = caps
                    .as_ref()
                    .and_then(|r| r.as_ref().ok())
                    .is_some_and(|c| c.passkeys_enabled);
                let caps_loaded = caps.is_some();
                let caps_error = matches!(caps, Some(Err(_)));
                let device_ok = browser_ok.get();

                let badge = if !caps_loaded {
                    "Loading"
                } else if passkeys_on && device_ok {
                    "Available"
                } else {
                    "Not ready"
                };
                let badge_on = passkeys_on && device_ok;
                let deployment_label = if !caps_loaded {
                    "Checking…"
                } else if caps_error {
                    "Error"
                } else if passkeys_on {
                    "Enabled"
                } else {
                    "Disabled"
                };

                view! {
                    <div class="passkey-overview">
                        <section class="panel passkey-status-panel">
                            <div class="mfa-status-head">
                                <div>
                                    <p class="section-label">"Phishing-resistant sign-in"</p>
                                    <h2>"Passkeys"</h2>
                                    <p class="passkey-lede">
                                        "A passkey lets you sign in with the biometrics or PIN already on this device. It cannot be phished like a password."
                                    </p>
                                </div>
                                {if badge_on {
                                    view! { <span class="mfa-badge mfa-badge-on">{badge}</span> }.into_any()
                                } else {
                                    view! { <span class="mfa-badge mfa-badge-off">{badge}</span> }.into_any()
                                }}
                            </div>
                            <dl class="kv mfa-status-kv">
                                <dt>"Account"</dt>
                                <dd>{session_email.clone()}</dd>
                                <dt>"Deployment"</dt>
                                <dd>{deployment_label}</dd>
                                <dt>"This browser"</dt>
                                <dd>{if device_ok { "Supports WebAuthn" } else { "No passkey API" }}</dd>
                            </dl>
                        </section>

                        {if !caps_loaded {
                            view! {
                                <section class="panel">
                                    <p class="result-line">"Loading passkey settings…"</p>
                                </section>
                            }.into_any()
                        } else if !passkeys_on {
                            view! {
                                <section class="panel">
                                    <p class="section-label">"Operator note"</p>
                                    <h2>"Passkeys are off for this deployment"</h2>
                                    <p class="passkey-lede">
                                        "Turn them on with AUTH_ENABLE_PASSKEYS=true, then set AUTH_PASSKEY_RP_ID and AUTH_PASSKEY_ORIGIN to match your public site origin (for local: localhost and http://localhost:3008 — not 127.0.0.1)."
                                    </p>
                                    <ol class="mfa-steps-preview">
                                        <li><strong>"RP ID"</strong>" must match the site host (no port). Use localhost, never an IP address."</li>
                                        <li><strong>"Origin"</strong>" must match the exact browser origin including scheme and port."</li>
                                        <li><strong>"HTTPS"</strong>" is required outside localhost."</li>
                                    </ol>
                                    <div class="button-row">
                                        <a class="secondary-button" href="/account/mfa">"Use authenticator app instead"</a>
                                        <a class="secondary-button" href="/account/password">"Password settings"</a>
                                    </div>
                                </section>
                            }.into_any()
                        } else if !device_ok {
                            view! {
                                <section class="panel">
                                    <p class="section-label">"Device"</p>
                                    <h2>"This browser cannot create passkeys"</h2>
                                    <p class="passkey-lede">
                                        "Try a current Chrome, Safari, Edge, or Firefox build, or open this page on a phone that supports platform authenticators."
                                    </p>
                                    <div class="button-row">
                                        <a class="secondary-button" href="/account/mfa">"Set up authenticator app"</a>
                                        <a class="secondary-button" href="/login">"Password sign-in"</a>
                                    </div>
                                </section>
                            }.into_any()
                        } else {
                            view! {
                                <section class="panel">
                                    <p class="section-label">"Add to this account"</p>
                                    <h2>"Create a passkey"</h2>
                                    <ol class="mfa-steps-preview">
                                        <li><strong>"Start"</strong>" registration for "{session_email.clone()}"."</li>
                                        <li><strong>"Approve"</strong>" the system prompt (biometrics or security key)."</li>
                                        <li><strong>"Done"</strong>" — use passkey next time you sign in."</li>
                                    </ol>
                                    <button
                                        type="button"
                                        class="primary-button"
                                        disabled=move || start_pending.get() || verify_pending.get() || !can_register
                                        on:click=move |_| {
                                            set_client_error.set(None);
                                            start_action.dispatch(StartPasskeyRegistration {
                                                email: email_dispatch.clone(),
                                                redirect_url: Some("/account/passkeys".to_owned()),
                                            });
                                        }
                                    >"Create a passkey"</button>
                                    <p class="passkey-hint">
                                        "Works with iCloud Keychain, Google Password Manager, 1Password, and hardware keys (YubiKey, etc.)."
                                    </p>
                                    <p class="error-banner" hidden=move || client_error.get().is_none()>
                                        {move || client_error.get().unwrap_or_default()}
                                    </p>
                                    <p class="error-banner" hidden=move || !matches!(start_value.get(), Some(Err(_)))>
                                        {move || match start_value.get() {
                                            Some(Err(error)) => server_error_text(error),
                                            _ => String::new(),
                                        }}
                                    </p>
                                </section>
                            }.into_any()
                        }}
                    </div>
                }.into_any()
            }}
        </div>
    }
}

/// Login-page optional block (unchanged gate).
#[island(lazy)]
fn OptionalPasskeyRegistration() -> impl IntoView {
    view! { <PasskeyManager /> }
}

#[island]
fn LogoutForm() -> impl IntoView {
    let action = ServerAction::<LogoutCurrentSession>::new();
    let pending = action.pending();
    let value = action.value();

    Effect::new(move |_| {
        if value.get().is_some_and(|result| result.is_ok()) {
            redirect_browser("/");
        }
    });

    view! {
        <div class="action-stack">
            <button
                type="button"
                class="secondary-button"
                disabled=move || pending.get()
                on:click=move |_| {
                    action.dispatch(LogoutCurrentSession {});
                }
            >
                "Log out"
            </button>
            <Show when=move || value.get().is_some()>
                <p class="result-line">{move || action_result_text(value.get())}</p>
            </Show>
        </div>
    }
}

#[island]
fn LogoutButton() -> impl IntoView {
    let action = ServerAction::<LogoutCurrentSession>::new();
    let pending = action.pending();
    let value = action.value();

    Effect::new(move |_| {
        if value.get().is_some_and(|result| result.is_ok()) {
            redirect_browser("/");
        }
    });

    view! {
        <button
            type="button"
            class="user-menu-signout"
            disabled=move || pending.get()
            on:click=move |_| {
                action.dispatch(LogoutCurrentSession {});
            }
        >
            "Sign out"
        </button>
    }
}

#[island]
fn SessionSummary() -> impl IntoView {
    let session = browser_load(get_current_session);

    view! {
        <section class="panel session-panel">
            <div class="session-panel-head">
                <div>
                    <p class="section-label">"Identity"</p>
                    <h2>"Current session"</h2>
                </div>
                <span
                    class="session-assurance"
                    hidden=move || !matches!(session.get(), Some(Ok(view)) if view.authenticated)
                >
                    {move || {
                        session
                            .get()
                            .and_then(Result::ok)
                            .map(|view| view.assurance.to_uppercase())
                            .unwrap_or_default()
                    }}
                </span>
            </div>
            <dl
                class="kv"
                hidden=move || !matches!(session.get(), Some(Ok(view)) if view.authenticated)
            >
                <dt>"Tenant"</dt>
                <dd class="mono-value">{move || session.get().and_then(Result::ok).and_then(|view| view.tenant_id).unwrap_or_else(|| "—".to_string())}</dd>
                <dt>"User"</dt>
                <dd class="mono-value">{move || session.get().and_then(Result::ok).and_then(|view| view.user_id).unwrap_or_else(|| "—".to_string())}</dd>
                <dt>"Email"</dt>
                <dd>{move || session.get().and_then(Result::ok).and_then(|view| view.primary_email).unwrap_or_else(|| "—".to_string())}</dd>
            </dl>
            <p
                class="result-line"
                hidden=move || matches!(session.get(), Some(Ok(view)) if view.authenticated)
            >
                {move || match session.get() {
                    None => "Loading session details".to_string(),
                    Some(Ok(_)) => "No active session".to_string(),
                    Some(Err(error)) => error.to_string(),
                }}
            </p>
        </section>
    }
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

#[island]
fn OAuthCallbackStatus() -> impl IntoView {
    let action = ServerAction::<CompleteOauthCallback>::new();
    let pending = action.pending();
    let value = action.value();

    view! {
        <section class="panel">
            <button
                type="button"
                class="secondary-button"
                disabled=move || pending.get()
                on:click=move |_| {
                    action.dispatch(CompleteOauthCallback {
                        provider_id: "unknown".to_string(),
                        code: None,
                        state: None,
                        redirect_url: Some(next_url()),
                    });
                }
            >
                "Complete callback"
            </button>
            <Show when=move || value.get().is_some()>
                <p class="result-line">{move || action_result_text(value.get())}</p>
            </Show>
        </section>
    }
}

#[component]
fn LoginRedirectLink() -> impl IntoView {
    view! {
        <a
            class="link-button"
            href=move || format!("/login?next={}", percent_encode_component(&next_url()))
        >
            "Sign in"
        </a>
    }
}

#[component]
fn ReturnToLoginLink() -> impl IntoView {
    view! { <a class="link-button" href="/login">"Return to sign in"</a> }
}

/// Page body only — shell chrome lives in `WorkspaceShell` / `AppLayout` and is reused.
/// Uses the wide `.page-grid` (orgs, admin, dashboard-adjacent).
fn page_shell(
    title: &'static str,
    subtitle: &'static str,
    children: impl IntoView + 'static,
) -> impl IntoView {
    view! {
        <section class="page-header workspace-page-header">
            <h1>{title}</h1>
            <span class="workspace-page-subtitle">{subtitle}</span>
        </section>
        <section class="page-grid">
            {children}
        </section>
    }
}

/// Narrow centered column for account + vault settings (matches profile ~640px).
/// Title and body share the same column so panels align with the page heading.
fn account_page_shell(
    title: &'static str,
    subtitle: &'static str,
    _active: &'static str,
    children: impl IntoView + 'static,
) -> impl IntoView {
    view! {
        <div class="account-page">
            <header class="account-page-header">
                <h1>{title}</h1>
                <p class="account-page-subtitle">{subtitle}</p>
            </header>
            <div class="account-page-body">
                {children}
            </div>
        </div>
    }
}

// The public shell is referenced by the browser route graph; the server-only
// component build can otherwise report it as dead code before route expansion.
#[allow(dead_code)]
fn public_page_shell(
    title: &'static str,
    subtitle: &'static str,
    children: impl IntoView + 'static,
) -> impl IntoView {
    view! {
        <div class="page">
            <header class="page-brand">
                <a class="page-brand-link" href="/" aria-label="wasi-auth home">
                    <span class="page-brand-mark" aria-hidden="true">"d"</span>
                    <span>
                        <strong>"wasi-auth"</strong>
                        <small>"ddd_cqrs_es fullstack"</small>
                    </span>
                </a>
                <span class="page-brand-status">
                    <span class="status-dot" aria-hidden="true"></span>
                    "Spin runtime"
                </span>
            </header>
            <section class="page-header">
                <p class="page-header-kicker">"wasi-auth / ddd_cqrs_es"</p>
                <h1>{title}</h1>
                <span>{subtitle}</span>
            </section>
            <section class="page-grid">
                {children}
            </section>
        </div>
    }
}

/// Marks active nav links. Island so it runs on the client and follows SPA navigations.
#[island]
fn WorkspaceNavActive() -> impl IntoView {
    Effect::new(move |_| {
        #[cfg(feature = "hydrate")]
        {
            use wasm_bindgen::closure::Closure;
            use wasm_bindgen::JsCast;

            let on_mark = Closure::wrap(Box::new(move |_event: web_sys::Event| {
                if let Some(window) = window() {
                    if let Ok(pathname) = window.location().pathname() {
                        mark_active_nav(&pathname);
                    }
                }
            }) as Box<dyn FnMut(_)>);
            if let Some(window) = window() {
                let _ = window.add_event_listener_with_callback(
                    "workspace-nav-mark",
                    on_mark.as_ref().unchecked_ref(),
                );
                on_mark.forget();
                bind_workspace_nav_active();
                if let Ok(pathname) = window.location().pathname() {
                    mark_active_nav(&pathname);
                }
            }
        }
    });
    view! { <span class="workspace-nav-active-marker" aria-hidden="true"></span> }
}

/// Desktop sidebar modes: full ↔ mini (rail toggle) and show ↔ hide (⌘/Ctrl+B).
#[island]
fn WorkspaceSidebarControls() -> impl IntoView {
    Effect::new(move |_| {
        #[cfg(feature = "hydrate")]
        {
            init_workspace_sidebar();
        }
    });
    view! { <span class="workspace-sidebar-controls" aria-hidden="true"></span> }
}

/// Top-bar workspace switcher: select org, jump to vault, create workspace.
#[island]
fn WorkspaceOrgSwitcher() -> impl IntoView {
    let orgs = browser_load(list_organizations);
    let session = browser_load(get_current_session);
    let select_action = ServerAction::<SelectOrganization>::new();
    let select_pending = select_action.pending();

    Effect::new(move |_| {
        if matches!(select_action.value().get(), Some(Ok(_))) {
            #[cfg(feature = "hydrate")]
            {
                if let Some(window) = window() {
                    let _ = window.location().reload();
                }
            }
        }
    });

    view! {
        <div class="org-switcher">
            {move || {
                let session = session.get();
                let orgs = orgs.get();
                match (session, orgs) {
                    (Some(Ok(sess)), Some(Ok(list))) if sess.authenticated => {
                        let active_id = sess.tenant_id.clone().filter(|s| !s.trim().is_empty());
                        let active = active_id.as_ref().and_then(|id| {
                            list.organizations.iter().find(|o| o.organization_id == *id).cloned()
                        });
                        let label = active
                            .as_ref()
                            .map(|o| o.name.clone())
                            .unwrap_or_else(|| {
                                if list.organizations.is_empty() {
                                    "No workspace".into()
                                } else {
                                    "Select workspace".into()
                                }
                            });
                        let monogram = active
                            .as_ref()
                            .map(|o| org_monogram(&o.name))
                            .unwrap_or_else(|| "W".into());
                        let vault_href = active
                            .as_ref()
                            .map(|o| {
                                if o.slug.is_empty() {
                                    "/account/vault".into()
                                } else {
                                    format!("/org/{}/vault", o.slug)
                                }
                            })
                            .unwrap_or_else(|| "/organizations".into());
                        let orgs_for_list = list.organizations.clone();

                        view! {
                            <details class="org-switcher-details">
                                <summary class="org-switcher-trigger" aria-label="Switch workspace">
                                    <span class="org-switcher-avatar" aria-hidden="true">{monogram}</span>
                                    <span class="org-switcher-meta">
                                        <span class="org-switcher-label">{label}</span>
                                        <span class="org-switcher-hint">"Workspace"</span>
                                    </span>
                                    <span class="org-switcher-caret" aria-hidden="true"></span>
                                </summary>
                                <div class="org-switcher-panel" role="menu">
                                    <p class="org-switcher-panel-label">"Workspaces"</p>
                                    <ul class="org-switcher-list">
                                        {orgs_for_list.into_iter().map(|org| {
                                            let id = org.organization_id.clone();
                                            let id_select = id.clone();
                                            let is_active = active_id.as_ref().is_some_and(|a| a == &id);
                                            let name = org.name.clone();
                                            let slug_line = if org.slug.is_empty() {
                                                String::new()
                                            } else {
                                                format!("/org/{}", org.slug)
                                            };
                                            view! {
                                                <li>
                                                    <button
                                                        type="button"
                                                        class="org-switcher-item"
                                                        class:is-active=is_active
                                                        role="menuitem"
                                                        disabled=move || select_pending.get() || is_active
                                                        on:click=move |_| {
                                                            if is_active { return; }
                                                            select_action.dispatch(SelectOrganization {
                                                                organization_id: id_select.clone(),
                                                            });
                                                        }
                                                    >
                                                        <span class="org-switcher-item-name">{name}</span>
                                                        <span class="org-switcher-item-meta">
                                                            {if is_active { "Active".into() } else { slug_line }}
                                                        </span>
                                                    </button>
                                                </li>
                                            }
                                        }).collect_view()}
                                    </ul>
                                    <div class="org-switcher-divider" aria-hidden="true"></div>
                                    <a class="org-switcher-link" href=vault_href role="menuitem">"Secret vault"</a>
                                    <a class="org-switcher-link" href="/organizations" role="menuitem">"Manage workspaces"</a>
                                    <a class="org-switcher-link" href="/onboarding/workspace?new=1" role="menuitem">"Create workspace"</a>
                                </div>
                            </details>
                        }.into_any()
                    }
                    (Some(Ok(_)), _) | (None, _) => view! {
                        <span class="org-switcher-fallback">"…"</span>
                    }.into_any(),
                    _ => view! {
                        <a class="org-switcher-fallback-link" href="/organizations">"Workspaces"</a>
                    }.into_any(),
                }
            }}
        </div>
    }
}

#[island(lazy)]
fn WorkspaceSystemNav() -> impl IntoView {
    let session = browser_load(get_current_session);

    view! {
        <div class="workspace-system-nav">
            {move || match session.get() {
                Some(Ok(session)) if session.authenticated && can_view_system_navigation(&session) => {
                    view! {
                        <p class="workspace-nav-label workspace-nav-label-secondary">"System"</p>
                        <a class="workspace-nav-link" href="/admin/health" data-nav="system" title="Health">
                            <span class="workspace-nav-icon" aria-hidden="true" data-icon="system"></span>
                            <span class="workspace-nav-text">"Health"</span>
                        </a>
                    }.into_any()
                }
                _ => view! {}.into_any(),
            }}
        </div>
    }
}

/// ChatGPT-style account flyout: avatar + email open a menu of settings + sign out.
/// Lives in the left rail foot so the main top bar stays clean.
/// Click-away / Escape dismiss is bound in `bindUserMenuDismiss` (workspace sidebar init).
#[island]
fn WorkspaceUserMenu() -> impl IntoView {
    let session = browser_load(get_current_session);

    #[cfg(feature = "hydrate")]
    Effect::new(move |_| {
        bind_user_menu_dismiss();
    });

    view! {
        <div class="user-menu">
            {move || match session.get() {
                Some(Ok(session)) if session.authenticated => {
                    let email = session
                        .primary_email
                        .clone()
                        .or_else(|| session.user_id.clone())
                        .unwrap_or_else(|| "Signed in".to_string());
                    let initial = email
                        .chars()
                        .next()
                        .map(|ch| ch.to_ascii_uppercase())
                        .unwrap_or('U');
                    view! {
                        <details class="user-menu-details">
                            <summary class="user-menu-trigger" aria-label="Account menu">
                                <span class="user-menu-avatar" aria-hidden="true">{initial.to_string()}</span>
                                <span class="user-menu-meta">
                                    <span class="user-menu-email">{email.clone()}</span>
                                    <span class="user-menu-hint">"Account"</span>
                                </span>
                                <span class="user-menu-caret" aria-hidden="true"></span>
                            </summary>
                            <div class="user-menu-panel" role="menu">
                                <p class="user-menu-panel-label">"Account"</p>
                                <a class="user-menu-item" href="/account/profile" role="menuitem">"Profile"</a>
                                <a class="user-menu-item" href="/account/password" role="menuitem">"Password"</a>
                                <a class="user-menu-item" href="/account/mfa" role="menuitem">"Authenticator (MFA)"</a>
                                <a class="user-menu-item" href="/account/passkeys" role="menuitem">"Passkeys"</a>
                                <a class="user-menu-item" href="/account/sessions" role="menuitem">"Sessions"</a>
                                <a class="user-menu-item" href="/account/providers" role="menuitem">"Providers"</a>
                                <div class="user-menu-divider" aria-hidden="true"></div>
                                <div class="user-menu-logout">
                                    <LogoutButton />
                                </div>
                            </div>
                        </details>
                    }.into_any()
                }
                Some(Ok(_)) => view! {
                    <a class="user-menu-signin" href="/login">"Sign in"</a>
                }.into_any(),
                Some(Err(_)) => view! {
                    <span class="user-menu-fallback">"Session unavailable"</span>
                }.into_any(),
                None => view! {
                    <span class="user-menu-fallback">"Loading…"</span>
                }.into_any(),
            }}
        </div>
    }
}

#[cfg(feature = "hydrate")]
fn mark_active_nav(pathname: &str) {
    let Some(document) = window().and_then(|window| window.document()) else {
        return;
    };

    let states = [
        (
            "[data-nav='overview']",
            pathname == "/dashboard",
        ),
        (
            "[data-nav='organizations']",
            pathname == "/organizations" || pathname.starts_with("/organizations/"),
        ),
        (
            "[data-nav='system']",
            pathname == "/admin" || pathname.starts_with("/admin/"),
        ),
    ];

    for (selector, active) in states {
        if let Ok(Some(element)) = document.query_selector(selector) {
            let _ = element.class_list().toggle_with_force("is-active", active);
        }
    }
}

fn can_view_system_navigation(session: &SessionView) -> bool {
    session.system_administrator && session.assurance == "aal2"
        || session
            .permissions
            .iter()
            .any(|permission| permission.starts_with("system.") || permission.starts_with("auth:"))
}

fn has_permission(session: &SessionView, permission: &str) -> bool {
    session.permissions.iter().any(|value| value == permission)
}

fn error_page_shell(
    title: &'static str,
    subtitle: &'static str,
    children: impl IntoView + 'static,
) -> impl IntoView {
    view! {
        <div class="error-page">
            <section class="error-card">
                <AuthBrand />
                <p class="auth-kicker">"Request interrupted"</p>
                <h1 class="error-title">{title}</h1>
                <p class="error-copy">{subtitle}</p>
                <div class="error-actions">{children}</div>
            </section>
        </div>
    }
}

fn selected_auth_error(
    register_mode: bool,
    login_result: Option<Result<LoginCompletionResponse, ServerFnError>>,
    register_result: Option<Result<LoginCompletionResponse, ServerFnError>>,
) -> Option<String> {
    let selected = if register_mode {
        register_result
    } else {
        login_result
    };
    match selected {
        Some(Err(error)) => Some(server_error_text(error)),
        _ => None,
    }
}

fn selected_action_error<T>(result: Option<Result<T, ServerFnError>>) -> Option<String> {
    match result {
        Some(Err(error)) => Some(server_error_text(error)),
        _ => None,
    }
}

fn validate_email_only(email: &str) -> Result<(), String> {
    if email.trim().is_empty() {
        return Err("Email is required.".to_string());
    }
    if !email.contains('@') || !email.contains('.') {
        return Err("Enter a valid email address.".to_string());
    }
    Ok(())
}

fn validate_login_form(email: &str, password: &str, register_mode: bool) -> Result<(), String> {
    validate_email_only(email)?;
    if password.is_empty() {
        return Err("Password is required.".to_string());
    }
    if register_mode && !(15..=128).contains(&password.chars().count()) {
        return Err("Password must contain 15 to 128 characters.".to_string());
    }
    Ok(())
}

fn server_error_text(error: ServerFnError) -> String {
    let text = error.to_string();
    text.strip_prefix("error running server function: ")
        .unwrap_or(&text)
        .to_string()
}

fn action_result_text<T>(result: Option<Result<T, ServerFnError>>) -> String {
    match result {
        Some(Ok(_)) => "Request accepted".to_string(),
        Some(Err(error)) => server_error_text(error),
        None => String::new(),
    }
}

fn optional_text(value: String) -> Option<String> {
    let value = value.trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}

fn browser_load<T, Fut, F>(load: F) -> ReadSignal<Option<T>>
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

#[cfg(feature = "hydrate")]
fn passkey_js_string(value: JsValue) -> Result<String, String> {
    value
        .as_string()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "Passkey response was not readable.".to_string())
}

#[cfg(feature = "hydrate")]
fn passkey_js_error(error: JsValue) -> String {
    // Prefer plain string throws from the JS layer. Fall back to Debug only
    // for unexpected DOMException / Error objects, then sanitize noise.
    let raw = error
        .as_string()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("{error:?}"));
    sanitize_passkey_client_error(&raw)
}

/// Strip wasm-bindgen / browser stack noise so the UI never shows
/// `JsValue(Error: … at createPasskeyCredential …)`.
fn sanitize_passkey_client_error(raw: &str) -> String {
    let mut message = raw.trim().to_owned();
    if message.is_empty() || message == "JsValue(undefined)" {
        return "Passkey prompt was cancelled or unavailable.".to_owned();
    }

    if let Some(inner) = message
        .strip_prefix("JsValue(")
        .and_then(|value| value.strip_suffix(')'))
    {
        message = inner.trim().to_owned();
    }
    if let Some(rest) = message.strip_prefix("Error: ") {
        message = rest.trim().to_owned();
    }
    // Drop trailing stack frames injected by wasm-bindgen Debug formatting.
    for marker in [
        " at createPasskeyCredential",
        " at getPasskeyCredential",
        "\n    at ",
    ] {
        if let Some(index) = message.find(marker) {
            message = message[..index].trim().to_owned();
        }
    }
    // Deduplicate doubled "Error: … Error: …" payloads.
    if let Some((first, _)) = message.split_once(" Error: ") {
        message = first.trim().to_owned();
    }

    let lower = message.to_ascii_lowercase();
    if lower.contains("notallowederror")
        || lower.contains("aborterror")
        || lower.contains("cancelled")
        || lower.contains("canceled")
        || lower.contains("timed out")
        || lower.contains("the operation either timed out")
        || message == "PASSKEY_CANCELLED"
    {
        return "Passkey prompt was cancelled.".to_owned();
    }
    if message.starts_with("JsValue(") {
        return "Passkey prompt was cancelled or unavailable.".to_owned();
    }
    message
}

fn is_passkey_cancel_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("cancelled")
        || lower.contains("canceled")
        || lower.contains("passkey_cancelled")
        || lower.contains("passkey_conditional_idle")
}

fn next_url() -> String {
    #[cfg(feature = "hydrate")]
    {
        if let Some(window) = window()
            && let Ok(search) = window.location().search()
        {
            let query = search.trim_start_matches('?');
            if let Some(encoded) = query
                .split('&')
                .find_map(|part| part.strip_prefix("next="))
            {
                let value = percent_decode_component(encoded);
                if value.starts_with('/')
                    && !value.starts_with("//")
                    && !value.starts_with("/login")
                {
                    return value;
                }
            }
        }
    }
    "/dashboard".to_string()
}

fn first_http_url_in_text(text: &str) -> Option<String> {
    for line in text.lines() {
        if let Some(url) = line
            .split_whitespace()
            .find(|part| part.starts_with("http://") || part.starts_with("https://"))
        {
            return Some(url.to_owned());
        }
    }
    None
}

fn percent_encode_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 3);
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                use std::fmt::Write as _;
                let _ = write!(out, "%{byte:02X}");
            }
        }
    }
    out
}

// Used from hydrate-only branches of next_url; keep available on SSR builds.
#[cfg_attr(not(feature = "hydrate"), allow(dead_code))]
fn percent_decode_component(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                if let (Some(high), Some(low)) = (
                    hex_nibble(bytes[index + 1]),
                    hex_nibble(bytes[index + 2]),
                ) {
                    out.push((high << 4) | low);
                    index += 3;
                    continue;
                }
                out.push(bytes[index]);
                index += 1;
            }
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg_attr(not(feature = "hydrate"), allow(dead_code))]
fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn one_time_token_from_url() -> Option<String> {
    #[cfg(feature = "hydrate")]
    {
        if let Some(window) = window()
            && let Ok(search) = window.location().search()
        {
            return search
                .trim_start_matches('?')
                .split('&')
                .find_map(|part| part.strip_prefix("token="))
                .map(ToOwned::to_owned)
                .filter(|value| !value.trim().is_empty());
        }
    }
    None
}

fn redirect_browser(url: &str) {
    #[cfg(feature = "hydrate")]
    {
        if let Some(window) = window() {
            let location = window.location();
            if location.replace(url).is_err() {
                let _ = location.set_href(url);
            }
        }
    }
    let _ = url;
}

#[cfg_attr(feature = "hydrate", allow(dead_code))]
fn set_page_status(status: http::StatusCode) {
    #[cfg(feature = "ssr")]
    {
        if let Some(resp) = use_context::<leptos_wasi::response::ResponseOptions>() {
            resp.set_status(status);
        }
    }
    let _ = status;
}

#[cfg(feature = "ssr")]
fn server_fn_error(error: crate::error::AuthStackError) -> ServerFnError {
    if error.is_client_error() {
        tracing::warn!(
            error = %error,
            error_code = error.public_code(),
            "auth server function rejected request"
        );
    } else {
        tracing::error!(
            error = %error,
            error_code = error.public_code(),
            "auth server function failed"
        );
    }
    error.server_fn_error()
}

#[cfg(feature = "ssr")]
fn current_session_id_from_cookie() -> Option<String> {
    use http::header::COOKIE;

    let parts = use_context::<http::request::Parts>()?;
    let cookie_header = parts.headers.get(COOKIE)?.to_str().ok()?;
    session_id_from_cookie_header(cookie_header)
}

#[cfg(feature = "ssr")]
fn server_fn_request_auth() -> crate::application::RequestAuth {
    if let Ok(context) = wasi_auth::leptos::current_verified_request_context() {
        return crate::application::RequestAuth::from_verified(context);
    }
    crate::application::RequestAuth::from_parts(current_session_id_from_cookie(), None, None)
}

#[cfg(feature = "ssr")]
fn session_id_from_cookie_header(cookie_header: &str) -> Option<String> {
    cookie_header.split(';').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        if matches!(name, "__Host-session" | "wasi_auth_dev_session") && !value.trim().is_empty() {
            Some(value.trim().to_string())
        } else {
            None
        }
    })
}

#[cfg(feature = "ssr")]
async fn set_session_cookie(response: &LoginCompletionResponse) {
    use http::HeaderValue;
    use http::header::SET_COOKIE;

    let Some(session_id) = response.session_id.as_deref() else {
        return;
    };
    let cookie_value = crate::application::session_cookie_header_value(
        session_id,
        Some(3600),
        crate::application::session_cookie_secure_enabled().await,
    );
    let Ok(cookie) = HeaderValue::from_str(&cookie_value) else {
        return;
    };
    if let Some(resp) = use_context::<leptos_wasi::response::ResponseOptions>() {
        resp.append_header(SET_COOKIE, cookie);
    }
}

#[cfg(any(feature = "ssr", test))]
fn browser_login_response(mut response: LoginCompletionResponse) -> LoginCompletionResponse {
    response.session_id = None;
    response.access_token = None;
    response.refresh_token = None;
    response
}

#[cfg(feature = "ssr")]
async fn clear_session_cookie() {
    use http::HeaderValue;
    use http::header::SET_COOKIE;

    let cookie_value = crate::application::expired_session_cookie_header_value(
        crate::application::session_cookie_secure_enabled().await,
    );
    let Ok(cookie) = HeaderValue::from_str(&cookie_value) else {
        return;
    };
    if let Some(resp) = use_context::<leptos_wasi::response::ResponseOptions>() {
        resp.append_header(SET_COOKIE, cookie);
    }
}

#[server(prefix = "/api/ui")]
pub async fn get_auth_capabilities() -> Result<AuthCapabilities, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::auth_capabilities()
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn register_email_password(
    email: String,
    password: String,
    redirect_url: Option<String>,
) -> Result<LoginCompletionResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response = crate::application::register_email_password(EmailPasswordRegisterRequest {
            email,
            password,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)?;
        set_session_cookie(&response).await;
        Ok(browser_login_response(response))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (email, password, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn complete_email_verification(
    token: String,
    redirect_url: Option<String>,
) -> Result<LoginCompletionResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response =
            crate::application::complete_email_verification(EmailVerificationCompleteRequest {
                token,
                redirect_url,
            })
            .await
            .map_err(server_fn_error)?;
        set_session_cookie(&response).await;
        Ok(browser_login_response(response))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (token, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn resend_email_verification(
    email: String,
    redirect_url: Option<String>,
) -> Result<AcceptedResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::resend_email_verification(EmailVerificationResendRequest {
            email,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (email, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn development_mail_capture_enabled() -> Result<bool, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        Ok(crate::auth_product::development_mail_capture_enabled().await)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn latest_development_mail(
    recipient: String,
    message_kind: String,
) -> Result<CapturedMailResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::latest_captured_mail(recipient, message_kind)
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (recipient, message_kind);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn login_email_password(
    email: String,
    password: String,
    redirect_url: Option<String>,
) -> Result<LoginCompletionResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response = crate::application::login_email_password(EmailPasswordLoginRequest {
            email,
            password,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)?;
        set_session_cookie(&response).await;
        Ok(browser_login_response(response))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (email, password, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn start_password_reset(
    email: String,
    redirect_url: Option<String>,
) -> Result<PasswordResetStartResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::start_password_reset(PasswordResetStartRequest {
            email,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (email, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn complete_password_reset(
    token: String,
    password: String,
    redirect_url: Option<String>,
) -> Result<LoginCompletionResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response = crate::application::complete_password_reset(PasswordResetCompleteRequest {
            token,
            password,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)?;
        set_session_cookie(&response).await;
        Ok(browser_login_response(response))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (token, password, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_auth_providers() -> Result<Vec<AuthProviderSummary>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_auth_providers()
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn get_current_session() -> Result<SessionView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::get_current_session_for(current_session_id_from_cookie())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn get_account_profile() -> Result<ProfileView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::get_account_profile(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn update_account_profile(
    first_name: String,
    last_name: String,
    display_name: String,
    username: String,
    is_public: bool,
    avatar_data_url: Option<String>,
) -> Result<ProfileView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::update_account_profile(
            ProfileUpdateRequest {
                first_name,
                last_name,
                display_name,
                username,
                is_public,
                avatar_data_url,
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (
            first_name,
            last_name,
            display_name,
            username,
            is_public,
            avatar_data_url,
        );
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn get_public_profile(username: String) -> Result<PublicProfileView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::get_public_profile(username)
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = username;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn get_dashboard_snapshot() -> Result<DashboardSnapshot, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::get_dashboard_snapshot(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

// Nested layout (u8 col_span, enums, tree) cannot use default PostUrl/serde_qs —
// it stringifies numbers ("3") and fails with "expected u8".
#[server(prefix = "/api/ui", input = Json)]
pub async fn save_dashboard_layout(
    layout: DashboardLayout,
) -> Result<DashboardLayout, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::save_dashboard_layout(
            crate::contracts::DashboardLayoutUpdate { layout },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = layout;
        unreachable!()
    }
}

// Includes numeric fields (cache_ttl_seconds); keep JSON for the same reason.
#[server(prefix = "/api/ui", input = Json)]
pub async fn upsert_dashboard_source(
    id: Option<String>,
    name: String,
    method: String,
    url: String,
    json_path: String,
    shape: String,
    cache_ttl_seconds: u32,
    body_template: Option<String>,
) -> Result<crate::contracts::DataSourceSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::upsert_dashboard_source(
            DataSourceUpsert {
                id,
                name,
                method,
                url,
                headers: Vec::new(),
                body_template,
                json_path,
                shape,
                cache_ttl_seconds,
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (id, name, method, url, json_path, shape, cache_ttl_seconds, body_template);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn delete_dashboard_source(source_id: String) -> Result<AcceptedResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::delete_dashboard_source(source_id, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = source_id;
        unreachable!()
    }
}

#[server(prefix = "/api/ui", input = Json)]
pub async fn create_dashboard_secret(
    org_slug: String,
    request: SecretCreateRequest,
) -> Result<crate::contracts::SecretSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::create_dashboard_secret(
            None,
            Some(org_slug),
            request,
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (org_slug, request);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn delete_dashboard_secret(
    org_slug: String,
    secret_id: String,
) -> Result<AcceptedResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::delete_dashboard_secret(
            None,
            Some(org_slug),
            secret_id,
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (org_slug, secret_id);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn reveal_dashboard_secret(
    org_slug: String,
    secret_id: String,
) -> Result<crate::contracts::SecretRevealResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::reveal_dashboard_secret(
            None,
            Some(org_slug),
            secret_id,
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (org_slug, secret_id);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_dashboard_secrets(
    org_slug: String,
) -> Result<Vec<crate::contracts::SecretSummary>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_dashboard_secrets(
            None,
            Some(org_slug),
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = org_slug;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn resolve_workspace_vault_target(
) -> Result<crate::contracts::OrganizationSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::resolve_workspace_vault_target(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn seed_dashboard_demos() -> Result<AcceptedResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::seed_dashboard_demos(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        unreachable!()
    }
}

#[server(prefix = "/api/ui", input = Json)]
pub async fn migrate_workspace_legacy_data(
    request: crate::contracts::WorkspaceLegacyMigrateRequest,
) -> Result<crate::contracts::WorkspaceLegacyMigrateReport, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::migrate_workspace_legacy_data(request, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = request;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn test_dashboard_http_source(
    source_id: String,
) -> Result<crate::contracts::HttpQueryResult, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::test_dashboard_http_source(source_id, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = source_id;
        unreachable!()
    }
}

#[server(prefix = "/api/ui", input = Json)]
pub async fn upsert_dashboard_resource(
    request: crate::contracts::ResourceUpsert,
) -> Result<crate::contracts::ResourceSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::upsert_dashboard_resource(request, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = request;
        unreachable!()
    }
}

#[server(prefix = "/api/ui", input = Json)]
pub async fn upsert_dashboard_query(
    request: crate::contracts::QueryUpsert,
) -> Result<crate::contracts::QuerySummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::upsert_dashboard_query(request, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = request;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn delete_dashboard_resource(
    resource_id: String,
) -> Result<AcceptedResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::delete_dashboard_resource(resource_id, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = resource_id;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn delete_dashboard_query(query_id: String) -> Result<AcceptedResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::delete_dashboard_query(query_id, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = query_id;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn run_dashboard_query(
    query_id: String,
) -> Result<crate::contracts::QueryResult, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::run_dashboard_query(query_id, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = query_id;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn dismiss_dashboard_notification(
    notification_id: String,
) -> Result<Vec<DashboardNotification>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::dismiss_dashboard_notification(
            notification_id,
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = notification_id;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn update_dashboard_note(
    widget_id: String,
    text: String,
) -> Result<DashboardLayout, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::update_dashboard_note(
            crate::contracts::DashboardNoteUpdate { widget_id, text },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (widget_id, text);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn require_authenticated_route() -> Result<SessionView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::require_authenticated_route_for(current_session_id_from_cookie())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn require_authorized_route(permission: String) -> Result<SessionView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::require_authorized_route_for(
            &permission,
            current_session_id_from_cookie(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = permission;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn start_passkey_registration(
    email: Option<String>,
    redirect_url: Option<String>,
) -> Result<PasskeyStartResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::start_passkey_registration(
            PasskeyStartRequest {
                email,
                redirect_url,
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (email, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn verify_passkey_registration(
    challenge_id: String,
    credential_json: String,
    redirect_url: Option<String>,
) -> Result<LoginCompletionResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response = crate::application::verify_passkey_registration(
            PasskeyVerifyRequest {
                challenge_id,
                credential_json,
                redirect_url,
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)?;
        set_session_cookie(&response).await;
        Ok(browser_login_response(response))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (challenge_id, credential_json, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn start_passkey_login(
    email: Option<String>,
    redirect_url: Option<String>,
) -> Result<PasskeyStartResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::start_passkey_login(PasskeyStartRequest {
            email,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (email, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn verify_passkey_login(
    challenge_id: String,
    credential_json: String,
    redirect_url: Option<String>,
) -> Result<LoginCompletionResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response = crate::application::verify_passkey_login(PasskeyVerifyRequest {
            challenge_id,
            credential_json,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)?;
        set_session_cookie(&response).await;
        Ok(browser_login_response(response))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (challenge_id, credential_json, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn start_oauth_login(
    provider_id: String,
    redirect_url: Option<String>,
) -> Result<OAuthStartResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::start_oauth_login(provider_id, redirect_url)
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (provider_id, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn complete_oauth_callback(
    provider_id: String,
    code: Option<String>,
    state: Option<String>,
    redirect_url: Option<String>,
) -> Result<LoginCompletionResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response = crate::application::complete_oauth_callback(OAuthCallbackRequest {
            provider_id,
            code,
            state,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)?;
        set_session_cookie(&response).await;
        Ok(browser_login_response(response))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (provider_id, code, state, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn logout_current_session() -> Result<LogoutResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response = crate::application::logout_session(current_session_id_from_cookie())
            .await
            .map_err(server_fn_error)?;
        clear_session_cookie().await;
        Ok(response)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn change_password(
    current_password: String,
    new_password: String,
) -> Result<AcceptedResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::change_password(
            PasswordChangeRequest {
                current_password,
                new_password,
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (current_password, new_password);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_account_sessions() -> Result<AccountSessionListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_sessions(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn revoke_account_session(session_id: String) -> Result<AcceptedResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let current_session = current_session_id_from_cookie();
        let response = crate::application::revoke_account_session(
            SessionRevokeRequest {
                session_id: session_id.clone(),
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)?;
        if current_session.as_deref() == Some(session_id.as_str()) {
            clear_session_cookie().await;
        }
        Ok(response)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = session_id;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn get_mfa_status() -> Result<MfaStatusResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::mfa_status(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn start_totp_enrollment() -> Result<MfaEnrollStartResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::start_totp_enrollment(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn confirm_totp_enrollment(
    code: String,
) -> Result<MfaEnrollConfirmResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::confirm_totp_enrollment(
            MfaCodeRequest { code },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = code;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn verify_totp_step_up(code: String) -> Result<SessionView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::verify_totp_step_up(MfaCodeRequest { code }, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = code;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn verify_recovery_code(code: String) -> Result<SessionView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::use_recovery_code_for_step_up(
            MfaCodeRequest { code },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = code;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn save_auth_provider(
    provider_id: String,
    enabled: bool,
) -> Result<AuthProviderSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::admin_save_provider(provider_id, enabled, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (provider_id, enabled);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn save_redirect_allowlist(redirects_json: String) -> Result<bool, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::save_redirect_allowlist(redirects_json, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = redirects_json;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_signing_keys() -> Result<SigningKeyListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_signing_keys(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn rotate_signing_key(
    kid: String,
    retire_previous: bool,
) -> Result<SigningKeyRotateResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::rotate_signing_key(
            SigningKeyRotateRequest {
                kid,
                retire_previous: Some(retire_previous),
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (kid, retire_previous);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn get_authorization_capabilities()
-> Result<AuthorizationCapabilitiesResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::authorization_capabilities()
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn list_organizations() -> Result<OrganizationListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_organizations(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn create_organization(
    name: String,
    slug: String,
) -> Result<crate::contracts::OrganizationSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::create_organization(
            OrganizationCreateRequest { name, slug },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (name, slug);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn select_organization(organization_id: String) -> Result<SessionView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::select_organization(
            crate::contracts::OrganizationSelectRequest { organization_id },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = organization_id;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_current_organization_members() -> Result<MembershipListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let organization_id = current_organization_id().await?;
        crate::application::list_members(organization_id, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn list_current_organization_invitations() -> Result<InvitationListResponse, ServerFnError>
{
    #[cfg(feature = "ssr")]
    {
        let organization_id = current_organization_id().await?;
        crate::application::list_invitations(organization_id, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn invite_current_organization_member(
    email: String,
    role_id: String,
) -> Result<crate::contracts::InvitationSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let organization_id = current_organization_id().await?;
        crate::application::invite_member(
            InvitationCreateRequest {
                organization_id,
                email,
                role_id,
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (email, role_id);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn accept_organization_invitation(
    token: String,
) -> Result<OrganizationSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::accept_invitation(
            InvitationAcceptRequest { token },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = token;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_current_organization_roles() -> Result<RoleListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let organization_id = current_organization_id().await?;
        crate::application::list_roles(organization_id, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn upsert_current_organization_role(
    role_id: String,
    name: String,
    permissions: Vec<String>,
) -> Result<crate::contracts::RoleSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let organization_id = current_organization_id().await?;
        crate::application::upsert_role(
            RoleUpsertRequest {
                organization_id,
                role_id,
                name,
                permissions,
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (role_id, name, permissions);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_current_organization_audit() -> Result<AuditEventListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let organization_id = current_organization_id().await?;
        crate::application::list_audit_events(
            Some(organization_id),
            0,
            100,
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn list_admin_users() -> Result<AdminUserListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_admin_users(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn get_admin_health() -> Result<HealthStatusResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::get_health(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn list_policy_versions() -> Result<PolicyVersionListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_policy_versions(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn publish_policy_version(
    policy_text: String,
    schema_text: String,
) -> Result<crate::contracts::PolicyVersionSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::publish_policy(
            PolicyPublishRequest {
                policy_text,
                schema_text,
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (policy_text, schema_text);
        unreachable!()
    }
}

#[cfg(feature = "ssr")]
async fn current_organization_id() -> Result<String, ServerFnError> {
    let session =
        crate::application::require_authenticated_route_for(current_session_id_from_cookie())
            .await
            .map_err(server_fn_error)?;
    session
        .tenant_id
        .filter(|organization_id| organization_id != "tenant:default")
        .ok_or_else(|| ServerFnError::ServerError("select an organization first".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_login_response_removes_browser_visible_tokens() {
        let response = LoginCompletionResponse {
            authenticated: true,
            redirect_url: "/dashboard".to_string(),
            session_id: Some("session_123".to_string()),
            access_token: Some("access-token".to_string()),
            refresh_token: Some("refresh-token".to_string()),
            expires_in_seconds: 3600,
        };

        let redacted = browser_login_response(response);

        assert!(redacted.authenticated);
        assert_eq!(redacted.redirect_url, "/dashboard");
        assert_eq!(redacted.expires_in_seconds, 3600);
        assert_eq!(redacted.session_id, None);
        assert_eq!(redacted.access_token, None);
        assert_eq!(redacted.refresh_token, None);
    }
}
