#!/usr/bin/env node

import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const appDir = resolve(scriptDir, "..");

function loadDotenvDefaults(path) {
  if (!existsSync(path)) {
    return;
  }
  const lines = readFileSync(path, "utf8").split(/\r?\n/);
  for (const line of lines) {
    if (!line || line.trimStart().startsWith("#") || !line.includes("=")) {
      continue;
    }
    const [rawKey, ...rest] = line.split("=");
    const key = rawKey.trim();
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(key) || process.env[key] !== undefined) {
      continue;
    }
    process.env[key] = rest.join("=");
  }
}

loadDotenvDefaults(resolve(appDir, ".env"));

const baseUrl =
  process.env.BASE_URL ||
  process.env.AUTH_PUBLIC_BASE_URL ||
  "http://127.0.0.1:3008";
const systemAccessToken = process.env.AUTH_SYSTEM_ACCESS_TOKEN || "";
const providerList = (process.env.OAUTH_PROVIDERS || process.env.OAUTH_PROVIDER || "google")
  .split(/\s+/)
  .map((provider) => provider.trim().toLowerCase())
  .filter(Boolean);
const timeoutMs = Number(process.env.LIVE_OAUTH_TIMEOUT_MS || 300000);
const headless = process.env.LIVE_OAUTH_HEADLESS === "true";
const slowMo = Number(process.env.LIVE_OAUTH_SLOW_MO_MS || 0);
const expectedEmail = process.env.EXPECTED_EMAIL || "";

async function loadPlaywright() {
  try {
    return await import("playwright");
  } catch (error) {
    console.error("Playwright is required for live OAuth browser smoke checks.");
    console.error("Run with: npm install && npm exec -- playwright install chromium");
    throw error;
  }
}

function url(path) {
  return new URL(path, baseUrl).toString();
}

function assertSupportedProvider(provider) {
  if (!["google", "facebook", "apple"].includes(provider)) {
    throw new Error(`Unsupported OAuth provider "${provider}". Use google, facebook, or apple.`);
  }
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
      `${eventType} did not advance after provider callback; before=${beforeCount}, after=${afterCount}`,
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

async function fetchStorageStatus() {
  if (!systemAccessToken) {
    throw new Error("AUTH_SYSTEM_ACCESS_TOKEN is required for live OAuth browser storage evidence.");
  }
  return fetchJson("/api/auth/storage/status", {
    headers: { authorization: `Bearer ${systemAccessToken}` },
  });
}

async function providerLabel(provider) {
  const capabilities = await fetchJson("/api/auth/capabilities");
  if (capabilities.oauth_enabled !== true) {
    throw new Error("OAuth is not enabled. Set AUTH_ENABLE_OAUTH=true and restart the Spin app.");
  }
  const found = capabilities.providers?.find(
    (candidate) => candidate.provider_id === provider && candidate.enabled === true,
  );
  if (!found) {
    throw new Error(`OAuth provider "${provider}" is not enabled in /api/auth/capabilities.`);
  }
  return found.display_name || provider;
}

async function assertSession(cookieHeader, provider) {
  const session = await fetchJson("/api/auth/session", {
    headers: { cookie: cookieHeader },
  });
  if (session.authenticated !== true) {
    throw new Error(`${provider}: /api/auth/session is not authenticated`);
  }
  if (expectedEmail && session.primary_email !== expectedEmail) {
    throw new Error(
      `${provider}: expected session email ${expectedEmail}, got ${session.primary_email}`,
    );
  }
  if (!session.primary_email) {
    throw new Error(`${provider}: /api/auth/session did not include primary_email`);
  }

  const dashboard = await fetch(url("/dashboard"), {
    headers: { cookie: cookieHeader },
    redirect: "manual",
  });
  if (!dashboard.ok) {
    throw new Error(`${provider}: dashboard check failed with ${dashboard.status}`);
  }
}

function appendJsonMode(rawUrl) {
  const replayUrl = new URL(rawUrl);
  replayUrl.searchParams.set("format", "json");
  return replayUrl.toString();
}

async function assertCallbackReplayRejected(rawUrl, provider) {
  if (!rawUrl) {
    throw new Error(`${provider}: browser did not hit the OAuth callback endpoint`);
  }

  const response = await fetch(appendJsonMode(rawUrl), { redirect: "manual" });
  if (response.status !== 409) {
    throw new Error(`${provider}: replayed OAuth callback returned ${response.status}, expected 409`);
  }
  const body = await response.json().catch(() => ({}));
  if (body?.error?.code !== "conflict") {
    throw new Error(`${provider}: replayed OAuth callback did not return conflict error`);
  }
}

async function runProvider(browser, provider) {
  assertSupportedProvider(provider);
  const label = await providerLabel(provider);
  const beforeStorage = await fetchStorageStatus();
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

  try {
    console.log(`fullstack-app live OAuth browser: starting ${provider}`);
    await page.goto(url("/login?next=/dashboard"), { waitUntil: "domcontentloaded" });
    await page.getByRole("button", { name: label, exact: true }).click();

    await page.waitForURL(
      (currentUrl) => {
        const current = new URL(currentUrl);
        const expected = new URL(baseUrl);
        return current.origin !== expected.origin || current.pathname === "/dashboard";
      },
      { timeout: 15000 },
    );

    console.log(
      `fullstack-app live OAuth browser: complete ${provider} login in the opened browser; waiting for /dashboard`,
    );
    await page.waitForURL(
      (currentUrl) => {
        const current = new URL(currentUrl);
        const expected = new URL(baseUrl);
        return current.origin === expected.origin && current.pathname === "/dashboard";
      },
      { timeout: timeoutMs },
    );
    await page.waitForLoadState("domcontentloaded");

    const cookies = await context.cookies(baseUrl);
    const sessionCookie = cookies.find((cookie) =>
      ["__Host-session", "wasi_auth_dev_session"].includes(cookie.name),
    );
    if (!sessionCookie?.value) {
      throw new Error(`${provider}: browser did not receive a host-only session cookie`);
    }
    const cookieHeader = `${sessionCookie.name}=${sessionCookie.value}`;
    await assertSession(cookieHeader, provider);
    await assertCallbackReplayRejected(callbackUrls.at(-1), provider);

    const afterStorage = await fetchStorageStatus();
    for (const eventType of [
      "auth_oauth_state_created",
      "auth_oauth_state_consumed",
      "auth_external_identity_linked",
      "auth_session_issued",
    ]) {
      assertEventAdvanced(beforeStorage, afterStorage, eventType);
    }
    if (
      !afterStorage.checkpoints?.some(
        (checkpoint) =>
          checkpoint.projection_name === "auth.storage.read_models" &&
          Number(checkpoint.last_sequence || 0) > 0,
      )
    ) {
      throw new Error(`${provider}: auth projection checkpoint did not advance`);
    }

    console.log(`fullstack-app live OAuth browser: ${provider} passed`);
  } finally {
    await context.close();
  }
}

if (providerList.length === 0) {
  throw new Error("OAUTH_PROVIDERS must include at least one provider.");
}

const { chromium } = await loadPlaywright();
const browser = await chromium.launch({ headless, slowMo });

try {
  for (const provider of providerList) {
    await runProvider(browser, provider);
  }
  console.log(`fullstack-app live OAuth browser: passed for ${providerList.join(" ")}`);
} finally {
  await browser.close();
}
