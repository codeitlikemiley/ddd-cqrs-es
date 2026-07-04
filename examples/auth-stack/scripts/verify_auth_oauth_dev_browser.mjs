#!/usr/bin/env node

const baseUrl = process.env.BASE_URL || "http://127.0.0.1:3008";
const adminToken = process.env.AUTH_ADMIN_TOKEN || "";
const provider = (process.env.OAUTH_PROVIDER || "google").toLowerCase();

async function loadPlaywright() {
  try {
    return await import("playwright");
  } catch (error) {
    console.error("Playwright is required for OAuth development browser smoke checks.");
    console.error("Run with: npm install && npm exec -- playwright install chromium");
    throw error;
  }
}

function url(path) {
  return new URL(path, baseUrl).toString();
}

function eventCount(storage, eventType) {
  const entry = Array.isArray(storage.event_types)
    ? storage.event_types.find((item) => item.event_type === eventType)
    : undefined;
  return Number(entry?.count || 0);
}

function assertEventAdvanced(before, after, eventType) {
  const beforeCount = eventCount(before, eventType);
  const afterCount = eventCount(after, eventType);
  if (afterCount <= beforeCount) {
    throw new Error(
      `${eventType} did not advance after OAuth UI callback; before=${beforeCount}, after=${afterCount}`,
    );
  }
}

async function fetchJson(path, options = {}) {
  const response = await fetch(url(path), options);
  if (!response.ok) {
    throw new Error(`${path} failed with ${response.status}: ${await response.text()}`);
  }
  return response.json();
}

async function storageStatus() {
  if (!adminToken) {
    return null;
  }
  return fetchJson("/api/auth/storage/status", {
    headers: { "x-auth-admin-token": adminToken },
  });
}

function appendJsonMode(rawUrl) {
  const replayUrl = new URL(rawUrl);
  replayUrl.searchParams.set("format", "json");
  return replayUrl.toString();
}

async function assertCallbackReplayRejected(rawUrl) {
  if (!rawUrl) {
    throw new Error("OAuth development browser smoke did not hit the callback endpoint.");
  }
  const response = await fetch(appendJsonMode(rawUrl), { redirect: "manual" });
  if (response.status !== 409) {
    throw new Error(`Replayed OAuth callback returned ${response.status}, expected 409.`);
  }
  const body = await response.json().catch(() => ({}));
  if (body?.error?.code !== "conflict") {
    throw new Error("Replayed OAuth callback did not return conflict error.");
  }
}

const capabilities = await fetchJson("/api/auth/capabilities");
if (capabilities.oauth_enabled !== true) {
  throw new Error("OAuth development browser smoke requires AUTH_ENABLE_OAUTH=true.");
}
const providerSummary = capabilities.providers?.find(
  (candidate) => candidate.provider_id === provider && candidate.enabled === true,
);
if (!providerSummary) {
  throw new Error(`OAuth provider "${provider}" is not enabled in /api/auth/capabilities.`);
}

const beforeStorage = await storageStatus();
const { chromium } = await loadPlaywright();
const browser = await chromium.launch({ headless: true });

try {
  const context = await browser.newContext({
    viewport: { width: 1280, height: 820 },
    baseURL: baseUrl,
  });
  const page = await context.newPage();
  const callbackUrls = [];
  page.on("request", (request) => {
    const current = new URL(request.url());
    const expected = new URL(baseUrl);
    if (
      current.origin === expected.origin &&
      current.pathname === `/api/auth/oauth/${provider}/callback`
    ) {
      callbackUrls.push(request.url());
    }
  });

  await page.goto(url("/login?next=/dashboard"), { waitUntil: "domcontentloaded" });
  await page.waitForLoadState("networkidle", { timeout: 10000 }).catch(() => {});
  await page.waitForTimeout(Number(process.env.OAUTH_DEV_HYDRATION_WAIT_MS || 1500));
  await page.getByRole("button", { name: providerSummary.display_name, exact: true }).click();
  try {
    await page.waitForURL(
      (currentUrl) => {
        const current = new URL(currentUrl);
        const expected = new URL(baseUrl);
        return current.origin === expected.origin && current.pathname === "/dashboard";
      },
      { timeout: 15000 },
    );
  } catch (error) {
    const bodyText = await page.locator("body").innerText({ timeout: 5000 }).catch(() => "");
    throw new Error(
      `OAuth UI did not navigate to /dashboard after clicking ${providerSummary.display_name}; current=${page.url()}; body=${bodyText.slice(0, 500)}; cause=${error}`,
    );
  }
  await page.waitForLoadState("domcontentloaded");

  const cookies = await context.cookies(baseUrl);
  const sessionCookie = cookies.find((cookie) => cookie.name === "ddd_auth_session");
  if (!sessionCookie?.value) {
    throw new Error("OAuth development browser smoke did not receive ddd_auth_session cookie.");
  }

  const session = await fetchJson("/api/auth/session", {
    headers: { cookie: `ddd_auth_session=${sessionCookie.value}` },
  });
  if (session.authenticated !== true || !session.primary_email) {
    throw new Error(`OAuth development session is invalid: ${JSON.stringify(session)}`);
  }

  const h1 = await page.locator("h1").first().innerText({ timeout: 5000 });
  if (h1.trim() !== "Dashboard") {
    throw new Error(`Expected dashboard after OAuth callback, got h1 "${h1.trim()}"`);
  }
  await assertCallbackReplayRejected(callbackUrls.at(-1));

  if (beforeStorage) {
    const afterStorage = await storageStatus();
    for (const eventType of [
      "auth_oauth_state_created",
      "auth_oauth_state_consumed",
      "auth_session_issued",
    ]) {
      assertEventAdvanced(beforeStorage, afterStorage, eventType);
    }
  }

  await context.close();
  console.log("auth-stack OAuth development browser smoke: passed");
} finally {
  await browser.close();
}
