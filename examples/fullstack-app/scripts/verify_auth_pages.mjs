#!/usr/bin/env node

const baseUrl = process.env.BASE_URL || "http://127.0.0.1:3008";
const expectSystemAdministrator =
  process.env.BROWSER_SMOKE_EXPECT_SYSTEM_ADMIN === "true";
const configuredSessionEmails = (process.env.BROWSER_SMOKE_EMAILS || "")
  .split(",")
  .map((email) => email.trim())
  .filter(Boolean);
let sessionEmailIndex = 0;
const desktop = { width: 1280, height: 720 };
const mobile = { width: 390, height: 844 };
const pendingWasmByPage = new WeakMap();

async function loadPlaywright() {
  try {
    return await import("playwright");
  } catch (error) {
    console.error("Playwright is required for browser smoke checks.");
    console.error(
      "Run with: npx -y -p playwright node examples/fullstack-app/scripts/verify_auth_pages.mjs",
    );
    throw error;
  }
}

function url(path) {
  return new URL(path, baseUrl).toString();
}

async function assertPage(page, path, expectedTitle) {
  await waitForPageWasm(page);
  await page.goto(url(path), { waitUntil: "domcontentloaded" });
  await page.waitForLoadState("networkidle", { timeout: 5000 }).catch(() => {});
  const state = await page.evaluate(() => ({
    h1: document.querySelector("h1")?.textContent?.trim() || "",
    overflowX:
      document.documentElement.scrollWidth >
      document.documentElement.clientWidth + 1,
    submitText:
      document.querySelector('button[type="submit"]')?.textContent?.trim() ||
      "",
  }));
  if (state.h1 !== expectedTitle) {
    throw new Error(
      `Expected ${path} h1 to be "${expectedTitle}", got "${state.h1}"`,
    );
  }
  if (state.overflowX) {
    throw new Error(`${path} has horizontal overflow at current viewport`);
  }
  await waitForPageWasm(page);
  return state;
}

async function waitForPageWasm(page) {
  const pendingWasm = pendingWasmByPage.get(page);
  if (!pendingWasm) return;

  const deadline = Date.now() + 15_000;
  let emptySince = null;
  while (Date.now() < deadline) {
    if (pendingWasm.size === 0) {
      emptySince ??= Date.now();
      if (Date.now() - emptySince >= 150) return;
    } else {
      emptySince = null;
    }
    await page.waitForTimeout(25);
  }
  throw new Error(
    `Timed out waiting for WASM before navigation: ${JSON.stringify([...pendingWasm])}`,
  );
}

async function assertVisibleText(page, expectedText) {
  await page
    .getByText(expectedText, { exact: true })
    .first()
    .waitFor({ state: "visible", timeout: 5000 });
}

async function assertAnyText(page, expectedTexts) {
  const bodyText = await page.locator("body").innerText({ timeout: 5000 });
  if (!expectedTexts.some((text) => bodyText.includes(text))) {
    throw new Error(
      `Expected page to include one of ${JSON.stringify(expectedTexts)}, got: ${bodyText}`,
    );
  }
}

async function assertIslandComponents(page, expectedPrefixes) {
  const components = await page.locator("leptos-island").evaluateAll((islands) =>
    islands.map((island) => island.getAttribute("data-component") || ""),
  );
  if (
    components.length !== expectedPrefixes.length ||
    expectedPrefixes.some(
      (prefix) => !components.some((component) => component.startsWith(prefix)),
    )
  ) {
    throw new Error(
      `Expected island components ${JSON.stringify(expectedPrefixes)}, got ${JSON.stringify(components)}`,
    );
  }
}

function assertSplitRequestState(requestedWasm, prefix, expected) {
  const loaded = [...requestedWasm].some((pathname) =>
    pathname.split("/").pop()?.startsWith(prefix),
  );
  if (loaded !== expected) {
    throw new Error(
      `${prefix} was ${loaded ? "loaded" : "not loaded"}; expected loaded=${expected}`,
    );
  }
}

async function waitForSplitWasm(page, requestedWasm, pendingWasm, prefix) {
  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline) {
    const requested = [...requestedWasm].some((pathname) =>
      pathname.split("/").pop()?.startsWith(prefix),
    );
    if (requested && pendingWasm.size === 0) {
      return;
    }
    await page.waitForTimeout(50);
  }
  throw new Error(
    `Timed out loading ${prefix}; pending WASM: ${JSON.stringify([...pendingWasm])}`,
  );
}

async function assertRedirect(page, path, expectedPathPrefix) {
  await waitForPageWasm(page);
  await page.goto(url(path), { waitUntil: "domcontentloaded" });
  const currentUrl = new URL(page.url());
  if (!currentUrl.pathname.startsWith(expectedPathPrefix)) {
    throw new Error(
      `Expected ${path} to redirect to ${expectedPathPrefix}, got ${page.url()}`,
    );
  }
  await waitForPageWasm(page);
}

async function capturedMail(email, kind) {
  let lastStatus = 0;
  for (let attempt = 0; attempt < 50; attempt += 1) {
    const response = await fetch(
      url(
        `/api/auth/dev/mail/latest?recipient=${encodeURIComponent(email)}&kind=${encodeURIComponent(kind)}`,
      ),
    );
    lastStatus = response.status;
    if (response.ok) return response.json();
    if (response.status !== 404) {
      throw new Error(
        `Mail capture failed with ${response.status}: ${await response.text()}`,
      );
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Mail worker did not deliver ${kind}; last status ${lastStatus}`);
}

async function createSessionCookie() {
  const email =
    configuredSessionEmails[sessionEmailIndex++] ||
    `browser-smoke-${Date.now()}-${sessionEmailIndex}@example.test`;
  const response = await fetch(url("/api/auth/password/register"), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      email,
      password: "browser-correct-123",
      redirect_url: "/dashboard",
    }),
  });
  if (!response.ok) {
    throw new Error(
      `Register request failed with ${response.status}: ${await response.text()}`,
    );
  }
  const body = await response.json();
  if (body.authenticated !== false || body.session_id !== null) {
    throw new Error(`Register response did not create a pending account`);
  }
  const mail = await capturedMail(email, "email-verification");
  const verificationUrl = new URL(mail.body_text, baseUrl);
  const token = verificationUrl.searchParams.get("token");
  if (!token) {
    throw new Error("Verification mail did not contain a one-time token");
  }
  const verificationResponse = await fetch(url("/api/auth/email/verify"), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ token, redirect_url: "/dashboard" }),
  });
  if (!verificationResponse.ok) {
    throw new Error(
      `Verification failed with ${verificationResponse.status}: ${await verificationResponse.text()}`,
    );
  }
  const verification = await verificationResponse.json();
  if (!verification.session_id) {
    throw new Error(`Verification response did not include a session_id`);
  }
  const parsed = new URL(baseUrl);
  return {
    email,
    sessionId: verification.session_id,
    cookie: {
      name: "wasi_auth_dev_session",
      value: verification.session_id,
      domain: parsed.hostname,
      path: "/",
      httpOnly: true,
      sameSite: "Lax",
    },
  };
}

function decodeBase32(value) {
  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
  let bits = 0;
  let buffer = 0;
  const output = [];
  for (const character of value.toUpperCase().replace(/=+$/u, "")) {
    const index = alphabet.indexOf(character);
    if (index < 0) throw new Error("TOTP secret is not valid Base32");
    buffer = (buffer << 5) | index;
    bits += 5;
    if (bits >= 8) {
      bits -= 8;
      output.push((buffer >> bits) & 0xff);
    }
  }
  return Buffer.from(output);
}

async function currentTotp(secretBase32) {
  const { createHmac } = await import("node:crypto");
  const counter = Math.floor(Date.now() / 1000 / 30);
  const message = Buffer.alloc(8);
  message.writeBigUInt64BE(BigInt(counter));
  const digest = createHmac("sha1", decodeBase32(secretBase32))
    .update(message)
    .digest();
  const offset = digest[digest.length - 1] & 0x0f;
  const binary =
    ((digest[offset] & 0x7f) << 24) |
    ((digest[offset + 1] & 0xff) << 16) |
    ((digest[offset + 2] & 0xff) << 8) |
    (digest[offset + 3] & 0xff);
  return String(binary % 1_000_000).padStart(6, "0");
}

async function jsonApi(path, sessionId, options = {}) {
  const response = await fetch(url(path), {
    method: options.method || "GET",
    headers: {
      cookie: `wasi_auth_dev_session=${sessionId}`,
      origin: new URL(baseUrl).origin,
      ...(options.body ? { "content-type": "application/json" } : {}),
      ...(options.csrf ? { "x-csrf-token": options.csrf } : {}),
    },
    body: options.body ? JSON.stringify(options.body) : undefined,
  });
  const text = await response.text();
  let body;
  try {
    body = JSON.parse(text);
  } catch {
    body = text;
  }
  return { response, body };
}

async function verifyMfaFlow(sessionId) {
  const csrfResult = await jsonApi("/api/auth/csrf", sessionId);
  if (!csrfResult.response.ok || !csrfResult.body.token) {
    throw new Error(`CSRF token request failed: ${JSON.stringify(csrfResult.body)}`);
  }
  const csrf = csrfResult.body.token;
  const start = await jsonApi("/api/auth/mfa/totp/enroll/start", sessionId, {
    method: "POST",
    csrf,
  });
  if (!start.response.ok || !start.body.secret_base32) {
    throw new Error(`TOTP enrollment start failed: ${JSON.stringify(start.body)}`);
  }
  const code = await currentTotp(start.body.secret_base32);
  const confirm = await jsonApi("/api/auth/mfa/totp/enroll/confirm", sessionId, {
    method: "POST",
    csrf,
    body: { code },
  });
  if (
    !confirm.response.ok ||
    confirm.body.assurance !== "aal2" ||
    confirm.body.recovery_codes?.length !== 10
  ) {
    throw new Error(`TOTP enrollment confirmation failed: ${JSON.stringify(confirm.body)}`);
  }
  const recoveryCode = confirm.body.recovery_codes[0];
  const firstUse = await jsonApi("/api/auth/mfa/recovery/verify", sessionId, {
    method: "POST",
    csrf,
    body: { code: recoveryCode },
  });
  if (!firstUse.response.ok) {
    throw new Error(`Recovery code verification failed: ${JSON.stringify(firstUse.body)}`);
  }
  const replay = await jsonApi("/api/auth/mfa/recovery/verify", sessionId, {
    method: "POST",
    csrf,
    body: { code: recoveryCode },
  });
  if (replay.response.ok) {
    throw new Error("A recovery code was accepted more than once");
  }
}

const { chromium } = await loadPlaywright();
const browser = await chromium.launch({ headless: true });
const unexpectedBrowserErrors = [];

try {
  for (const viewport of [desktop, mobile]) {
    const context = await browser.newContext({ viewport });
    const page = await context.newPage();
    const requestedWasm = new Set();
    const pendingWasm = new Set();
    pendingWasmByPage.set(page, pendingWasm);
    page.on("request", (request) => {
      const requestUrl = new URL(request.url());
      if (requestUrl.pathname.endsWith(".wasm")) {
        requestedWasm.add(requestUrl.pathname);
        pendingWasm.add(requestUrl.pathname);
      }
    });
    page.on("requestfinished", (request) => {
      const requestUrl = new URL(request.url());
      pendingWasm.delete(requestUrl.pathname);
    });
    page.on("console", (message) => {
      if (!["error", "warning"].includes(message.type())) {
        return;
      }
      if (message.text().startsWith("Failed to load resource: the server responded with a status of")) {
        return;
      }
      unexpectedBrowserErrors.push(
        `browser console ${message.type()}: ${message.text()}`,
      );
    });
    page.on("pageerror", (error) => {
      unexpectedBrowserErrors.push(`browser page error: ${error.message}`);
    });
    page.on("requestfailed", (request) => {
      const requestUrl = new URL(request.url());
      pendingWasm.delete(requestUrl.pathname);
      const reason = request.failure()?.errorText || "unknown error";
      if (reason === "net::ERR_ABORTED") {
        return;
      }
      unexpectedBrowserErrors.push(
        `browser request failed: ${request.method()} ${request.url()} ${reason}`,
      );
    });

    await assertPage(page, "/", "Production fullstack Rust");
    await assertIslandComponents(page, []);
    await assertPage(page, "/login", "Welcome back");
    await assertIslandComponents(page, [
      "ExistingSessionRedirect_",
      "EmailPasswordAuthForm_",
      "OptionalLoginMethods_",
    ]);
    const register = await assertPage(page, "/register", "Create your workspace");
    if (register.submitText !== "Create workspace") {
      throw new Error(
        `Expected /register submit text to be "Create workspace", got "${register.submitText}"`,
      );
    }
    await assertPage(page, "/forgot-password", "Recover access");
    await assertPage(
      page,
      "/reset-password?token=browser-smoke-token",
      "Choose a new password",
    );
    await assertPage(page, "/verify-email", "Verify your email");
    await assertVisibleText(
      page,
      "Open this page from the one-time link in your verification message.",
    );
    await assertPage(page, "/verify-email/pending", "Check your inbox");
    await assertPage(page, "/auth/required", "Authentication required");
    await assertPage(page, "/auth/forbidden", "Access denied");
    await assertPage(page, "/auth/session-expired", "Session expired");
    await assertPage(page, "/auth/passkey-unsupported", "Passkey unavailable");
    await assertPage(page, "/auth/callback/google", "Completing sign-in");
    await assertPage(page, "/auth/callback/google/error", "Sign-in failed");
    await assertRedirect(page, "/dashboard", "/auth/required");
    await assertRedirect(page, "/account/security", "/auth/required");
    await assertRedirect(page, "/admin/auth/signing-keys", "/auth/required");

    const parsed = new URL(baseUrl);
    await context.addCookies([
      {
        name: "wasi_auth_dev_session",
        value: "not-a-real-session",
        domain: parsed.hostname,
        path: "/",
        httpOnly: true,
        sameSite: "Lax",
      },
    ]);
    await assertRedirect(page, "/dashboard", "/auth/required");
    await context.clearCookies();

    const session = await createSessionCookie();
    await context.addCookies([session.cookie]);
    await assertPage(page, "/dashboard", "Dashboard");
    await assertVisibleText(page, session.email);
    await assertPage(page, "/account/security", "Account security");
    await assertPage(page, "/account/mfa", "Multi-factor authentication");
    await verifyMfaFlow(session.sessionId);
    assertSplitRequestState(
      requestedWasm,
      "split_authorization_policy_page_loader_",
      false,
    );
    if (expectSystemAdministrator) {
      await assertPage(page, "/admin/authorization/policy", "Authorization policy");
      await waitForSplitWasm(
        page,
        requestedWasm,
        pendingWasm,
        "split_authorization_policy_page_loader_",
      );
      assertSplitRequestState(
        requestedWasm,
        "split_authorization_policy_page_loader_",
        true,
      );
      await assertPage(page, "/admin/auth/signing-keys", "Signing keys");
      await assertPage(page, "/admin/auth/providers", "Auth providers");
      await assertPage(page, "/admin/auth/redirects", "Redirect allowlist");
    } else {
      await assertRedirect(page, "/admin/authorization/policy", "/auth/forbidden");
      assertSplitRequestState(
        requestedWasm,
        "split_authorization_policy_page_loader_",
        false,
      );
      await assertRedirect(page, "/admin/auth/signing-keys", "/auth/forbidden");
      await assertRedirect(page, "/admin/auth/providers", "/auth/forbidden");
      await assertRedirect(page, "/admin/auth/redirects", "/auth/forbidden");
      await assertAnyText(page, ["Access denied"]);
    }
    await assertRedirect(page, "/login", "/dashboard");
    await assertRedirect(page, "/login?next=/account/security", "/account/security");
    await assertRedirect(page, "/login?next=https://evil.example", "/dashboard");
    await assertRedirect(page, "/login?next=//evil.example", "/dashboard");
    await assertRedirect(page, "/register", "/dashboard");
    await assertRedirect(page, "/forgot-password", "/dashboard");
    await assertRedirect(page, "/reset-password?token=browser-smoke-token", "/dashboard");

    await assertPage(page, "/logout", "Log out");
    await page.getByRole("button", { name: "Log out" }).click();
    await assertVisibleText(page, "Request accepted");
    await assertRedirect(page, "/dashboard", "/auth/required");

    await waitForPageWasm(page);
    await page.waitForTimeout(100);
    await context.close();
  }

  if (unexpectedBrowserErrors.length > 0) {
    throw new Error(
      `Unexpected browser diagnostics:\n${unexpectedBrowserErrors.join("\n")}`,
    );
  }
  console.log("fullstack-app browser smoke: passed");
} finally {
  await browser.close();
}
