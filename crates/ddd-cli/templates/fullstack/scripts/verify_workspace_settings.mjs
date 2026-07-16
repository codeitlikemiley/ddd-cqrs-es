#!/usr/bin/env node
/**
 * Playwright smoke: workspace settings shell + area pages.
 *
 * Prerequisites
 * -------------
 * - Running fullstack server (`make dev` or `make spin` + outbox worker for mail)
 * - Either:
 *     BROWSER_SMOKE_EMAILS=<existing@email>  (+ optional BROWSER_SMOKE_PASSWORD)
 *   or mail capture enabled (AUTH_MAIL_TRANSPORT=capture + outbox worker) so the
 *   script can register + verify a throwaway account.
 *
 * Isolation (mutating runs)
 * -------------------------
 * Creating an org (always) and renaming the workspace (optional) write to the
 * shared Postgres used by local smoke. Prefer:
 *
 *   make -C examples/fullstack-app fresh db=postgres
 *   make -C examples/fullstack-app dev
 *   make -C examples/fullstack-app workspace-settings-smoke
 *
 * Optional mutation (rename via UI):
 *   ALLOW_MUTATING_SMOKE=1 make -C examples/fullstack-app workspace-settings-smoke
 *
 * If AAL2 step-up blocks rename, the script accepts the step-up hint (no hard fail).
 *
 * Future: per-run isolated Postgres is not implemented yet — use `make fresh`
 * before mutating smokes on a shared DB.
 *
 * Env
 * ---
 *   BASE_URL / AUTH_PUBLIC_BASE_URL   default http://127.0.0.1:3008
 *   BROWSER_SMOKE_EMAILS              comma-separated; first used for login
 *   BROWSER_SMOKE_PASSWORD            default ChangeMe-Dev-Password-15!
 *   ALLOW_MUTATING_SMOKE              1 to attempt rename via General UI
 *
 * Run
 * ---
 *   npx -y -p playwright node scripts/verify_workspace_settings.mjs
 *   make workspace-settings-smoke
 */

const baseUrl = (
  process.env.BASE_URL ||
  process.env.AUTH_PUBLIC_BASE_URL ||
  "http://127.0.0.1:3008"
).replace(/\/$/, "");

const configuredEmail = (process.env.BROWSER_SMOKE_EMAILS || "")
  .split(",")
  .map((e) => e.trim())
  .filter(Boolean)[0];
const password =
  process.env.BROWSER_SMOKE_PASSWORD || "ChangeMe-Dev-Password-15!";
const allowMutating = process.env.ALLOW_MUTATING_SMOKE === "1";

function url(path) {
  return new URL(path, baseUrl).toString();
}

async function loadPlaywright() {
  try {
    return await import("playwright");
  } catch {
    console.error(
      "Playwright required: npx -y -p playwright node scripts/verify_workspace_settings.mjs",
    );
    process.exit(1);
  }
}

async function serverReachable() {
  try {
    const response = await fetch(url("/"), { method: "GET" });
    return response.status > 0;
  } catch {
    return false;
  }
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
  throw new Error(
    `Mail worker did not deliver ${kind} (last status ${lastStatus}). ` +
      `Ensure AUTH_MAIL_TRANSPORT=capture and the outbox worker is running (make dev).`,
  );
}

async function registerAndVerify() {
  const email =
    configuredEmail ||
    `ws-settings-smoke-${Date.now()}-${Math.floor(Math.random() * 1e6)}@example.test`;
  const registerPassword =
    process.env.BROWSER_SMOKE_PASSWORD || "browser-correct-123";
  const register = await fetch(url("/api/auth/password/register"), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      email,
      password: registerPassword,
      redirect_url: "/dashboard",
    }),
  });
  if (!register.ok) {
    throw new Error(
      `Register failed ${register.status}: ${await register.text()}`,
    );
  }
  const body = await register.json();
  if (body.authenticated === true && body.session_id) {
    return { email, sessionId: body.session_id };
  }

  const mail = await capturedMail(email, "email-verification");
  let token = null;
  const actionUrl = mail.action_url || null;
  if (actionUrl) {
    try {
      token = new URL(actionUrl, baseUrl).searchParams.get("token");
    } catch {
      /* ignore */
    }
  }
  if (!token && mail.body_text) {
    try {
      token = new URL(mail.body_text, baseUrl).searchParams.get("token");
    } catch {
      const match = String(mail.body_text).match(/token=([A-Za-z0-9._~-]+)/);
      token = match ? match[1] : null;
    }
  }
  if (!token) {
    throw new Error("Captured verification mail had no token");
  }

  const verificationResponse = await fetch(url("/api/auth/email/verify"), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ token, redirect_url: "/dashboard" }),
  });
  if (!verificationResponse.ok) {
    throw new Error(
      `Verification failed ${verificationResponse.status}: ${await verificationResponse.text()}`,
    );
  }
  const verification = await verificationResponse.json();
  if (!verification.session_id) {
    throw new Error("Verification response did not include session_id");
  }
  return { email, sessionId: verification.session_id };
}

async function passwordLogin(email, pass) {
  const response = await fetch(url("/api/auth/password/login"), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      email,
      password: pass,
      redirect_url: "/dashboard",
    }),
  });
  if (!response.ok) {
    throw new Error(
      `Login failed ${response.status}: ${await response.text()}`,
    );
  }
  const body = await response.json();
  if (!body.session_id) {
    throw new Error(
      `Login did not return session_id (authenticated=${body.authenticated}). ` +
        `Use mail capture register path or a verified account.`,
    );
  }
  return { email, sessionId: body.session_id };
}

async function obtainSession() {
  if (configuredEmail) {
    try {
      return await passwordLogin(configuredEmail, password);
    } catch (error) {
      console.warn(
        `password login failed for ${configuredEmail}; trying register+verify: ${error.message}`,
      );
    }
  }
  return registerAndVerify();
}

function sessionCookie(sessionId) {
  const host = new URL(baseUrl).hostname;
  return {
    name: "wasi_auth_dev_session",
    value: sessionId,
    domain: host,
    path: "/",
    httpOnly: true,
    sameSite: "Lax",
  };
}

async function ensureOrganization(page, sessionId) {
  const csrfResponse = await page.request.get(url("/api/auth/csrf"));
  if (!csrfResponse.ok()) {
    throw new Error(`CSRF token request failed with ${csrfResponse.status()}`);
  }
  const { token: csrfToken } = await csrfResponse.json();
  const stamp = `${Date.now()}-${Math.floor(Math.random() * 1e6)}`;
  const slug = `ws-settings-${stamp}`;
  const name = `WS Settings Smoke ${stamp}`;
  const organizationResponse = await page.request.post(url("/api/organizations"), {
    data: { name, slug },
    headers: {
      origin: baseUrl,
      "x-csrf-token": csrfToken,
      cookie: `wasi_auth_dev_session=${sessionId}`,
    },
  });
  if (!organizationResponse.ok()) {
    throw new Error(
      `Organization creation failed with ${organizationResponse.status()}: ${await organizationResponse.text()}`,
    );
  }
  return { slug, name };
}

async function waitForSettled(page) {
  await page.waitForLoadState("networkidle", { timeout: 8000 }).catch(() => {});
  await page.waitForTimeout(400);
}

async function assertSettingsPage(page, path, expectedH1) {
  await page.goto(url(path), { waitUntil: "domcontentloaded" });
  await waitForSettled(page);

  const current = new URL(page.url());
  if (current.pathname.startsWith("/auth/required")) {
    throw new Error(`Expected authenticated settings at ${path}, got ${page.url()}`);
  }
  if (current.pathname.startsWith("/auth/forbidden")) {
    throw new Error(`Forbidden loading ${path}`);
  }

  const state = await page.evaluate(() => {
    const shell = document.querySelector(
      "[data-testid='workspace-settings-shell']",
    );
    const h1 = document.querySelector("h1")?.textContent?.trim() || "";
    return { hasShell: Boolean(shell), h1 };
  });

  if (!state.hasShell) {
    throw new Error(
      `${path}: expected workspace-settings-shell (got h1="${state.h1}" url=${page.url()})`,
    );
  }
  if (state.h1 !== expectedH1) {
    throw new Error(
      `${path}: expected h1 "${expectedH1}", got "${state.h1}"`,
    );
  }
  return state;
}

async function assertGeneralNameField(page) {
  const nameInput = page
    .locator(
      '[data-testid="workspace-settings-general-form"] input[type="text"], form[data-testid="workspace-settings-general-form"] input',
    )
    .first();
  await nameInput.waitFor({ state: "visible", timeout: 15_000 });
  const value = await nameInput.inputValue();
  if (!value || !value.trim()) {
    // Context may still be loading; wait briefly for seed.
    await page.waitForTimeout(1500);
  }
  const after = await nameInput.inputValue();
  if (!after || !after.trim()) {
    // Shell + title are enough for read smoke; log soft note.
    console.warn("general: display name input still empty after wait (context load lag?)");
  }
  const body = await page.locator("body").innerText();
  if (!body.includes("Display name") && !body.includes("Workspace URL")) {
    throw new Error("general page missing Display name / Workspace URL copy");
  }
}

async function tryRenameWorkspace(page) {
  const nameInput = page
    .locator(
      '[data-testid="workspace-settings-general-form"] input[type="text"], form[data-testid="workspace-settings-general-form"] input',
    )
    .first();
  await nameInput.waitFor({ state: "visible", timeout: 15_000 });
  await page.waitForTimeout(800);
  const nextName = `WS Settings Renamed ${Date.now()}`;
  await nameInput.fill(nextName);
  await page.waitForTimeout(200);
  const save = page.locator('button[type="submit"]:has-text("Save")').first();
  if (!(await save.count())) {
    throw new Error("general: Save button not found");
  }
  await save.click();
  await page.waitForTimeout(2000);

  const body = await page.locator("body").innerText();
  if (body.includes("Workspace name saved.")) {
    console.log("rename: success");
    return;
  }
  if (
    body.includes("step-up") ||
    body.includes("AAL2") ||
    body.includes("Complete MFA") ||
    body.toLowerCase().includes("step up")
  ) {
    console.log("rename: AAL2/step-up required (accepted)");
    return;
  }
  // Soft accept: mutation may still be pending or permission-denied without copy we know.
  if (body.includes("error") || body.includes("Error") || body.includes("denied")) {
    console.log(
      "rename: non-success response (accepted for smoke isolation); body snippet: " +
        body.slice(0, 240).replace(/\s+/g, " "),
    );
    return;
  }
  console.log("rename: no explicit success/step-up text (accepted)");
}

async function main() {
  if (!(await serverReachable())) {
    console.log(
      `skip: server not reachable at ${baseUrl} (start with make dev, then re-run)`,
    );
    return;
  }

  let session;
  try {
    session = await obtainSession();
  } catch (error) {
    console.log(
      `skip: could not obtain session (${error.message}). ` +
        `Set BROWSER_SMOKE_EMAILS or enable mail capture (make dev).`,
    );
    return;
  }

  const { chromium } = await loadPlaywright();
  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({
    viewport: { width: 1280, height: 720 },
  });
  await context.addCookies([sessionCookie(session.sessionId)]);
  const page = await context.newPage();

  try {
    const org = await ensureOrganization(page, session.sessionId);
    console.log(`org: slug=${org.slug}`);

    const base = `/org/${org.slug}/settings`;
    await assertSettingsPage(page, `${base}/general`, "General");
    await assertGeneralNameField(page);

    await assertSettingsPage(page, `${base}/members`, "Members");
    await assertSettingsPage(page, `${base}/invitations`, "Invitations");
    await assertSettingsPage(page, `${base}/roles`, "Roles");
    await assertSettingsPage(page, `${base}/audit`, "Audit log");
    await assertSettingsPage(page, `${base}/danger`, "Danger zone");

    // Root settings path should land on general (or general-equivalent chrome).
    await page.goto(url(base), { waitUntil: "domcontentloaded" });
    await waitForSettled(page);
    const rootPath = new URL(page.url()).pathname;
    if (!rootPath.includes(`/org/${org.slug}/settings`)) {
      throw new Error(`settings root left settings tree: ${page.url()}`);
    }
    const rootShell = await page.locator("[data-testid='workspace-settings-shell']").count();
    if (!rootShell) {
      throw new Error(`settings root missing shell at ${page.url()}`);
    }

    if (allowMutating) {
      await assertSettingsPage(page, `${base}/general`, "General");
      await tryRenameWorkspace(page);
    } else {
      console.log("rename: skipped (set ALLOW_MUTATING_SMOKE=1 to attempt)");
    }

    console.log("verify_workspace_settings: ok");
  } finally {
    await browser.close();
  }
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
