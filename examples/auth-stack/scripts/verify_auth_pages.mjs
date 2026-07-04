#!/usr/bin/env node

const baseUrl = process.env.BASE_URL || "http://127.0.0.1:3008";
const desktop = { width: 1280, height: 720 };
const mobile = { width: 390, height: 844 };

async function loadPlaywright() {
  try {
    return await import("playwright");
  } catch (error) {
    console.error("Playwright is required for browser smoke checks.");
    console.error(
      "Run with: npx -y -p playwright node examples/auth-stack/scripts/verify_auth_pages.mjs",
    );
    throw error;
  }
}

function url(path) {
  return new URL(path, baseUrl).toString();
}

async function assertPage(page, path, expectedTitle) {
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
  return state;
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

async function assertRedirect(page, path, expectedPathPrefix) {
  await page.goto(url(path), { waitUntil: "domcontentloaded" });
  const currentUrl = new URL(page.url());
  if (!currentUrl.pathname.startsWith(expectedPathPrefix)) {
    throw new Error(
      `Expected ${path} to redirect to ${expectedPathPrefix}, got ${page.url()}`,
    );
  }
}

async function createSessionCookie() {
  const email = `browser-smoke-${Date.now()}@example.test`;
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
  if (!body.session_id) {
    throw new Error(`Register response did not include a session_id`);
  }
  const parsed = new URL(baseUrl);
  return {
    email,
    sessionId: body.session_id,
    cookie: {
      name: "ddd_auth_session",
      value: body.session_id,
      domain: parsed.hostname,
      path: "/",
      httpOnly: true,
      sameSite: "Lax",
    },
  };
}

const { chromium } = await loadPlaywright();
const browser = await chromium.launch({ headless: true });

try {
  for (const viewport of [desktop, mobile]) {
    const context = await browser.newContext({ viewport });
    const page = await context.newPage();

    await assertPage(page, "/login", "Welcome back");
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
        name: "ddd_auth_session",
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
    await assertPage(page, "/admin/authz/check", "Authorization check");
    await assertRedirect(page, "/admin/auth/signing-keys", "/auth/forbidden");
    await assertRedirect(page, "/admin/auth/providers", "/auth/forbidden");
    await assertRedirect(page, "/admin/auth/redirects", "/auth/forbidden");
    await assertAnyText(page, ["Access denied"]);
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

    await context.close();
  }

  console.log("auth-stack browser smoke: passed");
} finally {
  await browser.close();
}
