#!/usr/bin/env node
/**
 * Full-route browser audit (load + network + console).
 *
 * Not a substitute for make browser-smoke / workspace-settings-smoke — those are
 * the checked-in regression smokes. This script walks essentially every UI route
 * under an authenticated owner session and records:
 *   - final URL / h1
 *   - /api/ui/* status >= 400
 *   - console errors
 *   - body text matches for known failure phrases
 *
 * Usage (server already running):
 *   BASE_URL=http://localhost:3008 node scripts/audit_all_pages.mjs
 */

import { chromium } from "playwright";

const baseUrl = (
  process.env.BASE_URL ||
  process.env.AUTH_PUBLIC_BASE_URL ||
  "http://localhost:3008"
).replace(/\/$/, "");

const password = process.env.BROWSER_SMOKE_PASSWORD || "browser-correct-123";
const email =
  process.env.BROWSER_SMOKE_EMAILS?.split(",")[0]?.trim() ||
  `audit-${Date.now()}@example.test`;

function url(path) {
  return new URL(path, baseUrl).toString();
}

async function ensureAccount() {
  const register = await fetch(url("/api/auth/password/register"), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      email,
      password,
      redirect_url: "/dashboard",
    }),
  });
  // ignore non-ok if already exists
  await register.text();

  // Best-effort activate pending accounts via local psql if available
  // (dev-only audit; optional).
  try {
    const { spawnSync } = await import("node:child_process");
    spawnSync(
      "psql",
      [
        "-h",
        "127.0.0.1",
        "-p",
        "54329",
        "-U",
        "wasi_auth",
        "-d",
        "wasi_auth",
        "-c",
        `UPDATE auth_users SET status='active' WHERE primary_email='${email.replace(/'/g, "''")}';`,
      ],
      {
        env: { ...process.env, PGPASSWORD: "wasi_auth_dev" },
        stdio: "ignore",
      },
    );
  } catch {
    /* optional */
  }

  const login = await fetch(url("/api/auth/password/login"), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ email, password }),
  });
  const body = await login.json();
  if (!login.ok || !body.session_id) {
    throw new Error(`login failed: ${login.status} ${JSON.stringify(body)}`);
  }
  return body.session_id;
}

async function csrf(sessionId) {
  const response = await fetch(url("/api/auth/csrf"), {
    headers: { cookie: `wasi_auth_dev_session=${sessionId}` },
  });
  const body = await response.json();
  return body.token || body.csrf_token || "";
}

async function ensureOrg(sessionId) {
  const session = await fetch(url("/api/auth/session"), {
    headers: { cookie: `wasi_auth_dev_session=${sessionId}` },
  }).then((r) => r.json());
  if (session.tenant_id) {
    // Resolve slug
    const orgs = await fetch(url("/api/organizations"), {
      headers: { cookie: `wasi_auth_dev_session=${sessionId}` },
    }).then((r) => r.json());
    const list = orgs.organizations || orgs || [];
    const match =
      list.find?.((o) => o.organization_id === session.tenant_id) || list[0];
    return {
      orgId: session.tenant_id,
      slug: match?.slug || "workspace",
    };
  }
  const token = await csrf(sessionId);
  const created = await fetch(url("/api/organizations"), {
    method: "POST",
    headers: {
      "content-type": "application/json",
      cookie: `wasi_auth_dev_session=${sessionId}`,
      origin: baseUrl,
      "x-csrf-token": token,
    },
    body: JSON.stringify({ name: `Audit Space ${Date.now()}` }),
  }).then((r) => r.json());
  if (!created.organization_id) {
    throw new Error(`create org failed: ${JSON.stringify(created)}`);
  }
  await fetch(url("/api/organizations/select"), {
    method: "POST",
    headers: {
      "content-type": "application/json",
      cookie: `wasi_auth_dev_session=${sessionId}`,
      origin: baseUrl,
      "x-csrf-token": token,
    },
    body: JSON.stringify({ organization_id: created.organization_id }),
  });
  return { orgId: created.organization_id, slug: created.slug };
}

function routesForSlug(slug) {
  return [
    // guest / public
    { path: "/", guestOk: true },
    { path: "/login", guestOk: true },
    { path: "/register", guestOk: true },
    { path: "/forgot-password", guestOk: true },
    { path: "/reset-password", guestOk: true },
    { path: "/verify-email/pending", guestOk: true },
    { path: "/verify-email/resend", guestOk: true },
    { path: "/auth/required", guestOk: true },
    { path: "/auth/forbidden", guestOk: true },
    { path: "/auth/session-expired", guestOk: true },
    { path: "/auth/passkey-unsupported", guestOk: true },
    // authenticated product
    { path: "/dashboard" },
    { path: "/organizations" },
    { path: "/onboarding/workspace" },
    { path: "/account/profile" },
    { path: "/account/password" },
    { path: "/account/providers" },
    { path: "/account/passkeys" },
    { path: "/account/mfa" },
    { path: "/account/sessions" },
    { path: "/account/vault" },
    { path: `/org/${slug}/vault` },
    { path: `/org/${slug}/settings` },
    { path: `/org/${slug}/settings/general` },
    { path: `/org/${slug}/settings/members` },
    { path: `/org/${slug}/settings/invitations` },
    { path: `/org/${slug}/settings/roles` },
    { path: `/org/${slug}/settings/audit` },
    { path: `/org/${slug}/settings/danger` },
    // legacy org routes
    { path: "/organizations/settings" },
    { path: "/organizations/members" },
    { path: "/organizations/invitations" },
    { path: "/organizations/roles" },
    { path: "/organizations/permissions" },
    { path: "/organizations/audit" },
    // admin (expect forbidden for non-sysadmin, not 500/store)
    { path: "/admin/users", allowForbidden: true },
    { path: "/admin/health", allowForbidden: true },
    { path: "/admin/policies", allowForbidden: true },
    { path: "/admin/auth/signing-keys", allowForbidden: true },
    { path: "/admin/auth/providers", allowForbidden: true },
    { path: "/admin/auth/redirects", allowForbidden: true },
    { path: "/admin/authorization/policy", allowForbidden: true },
    // 404
    { path: "/this-route-does-not-exist-audit", guestOk: true, expectNotFound: true },
  ];
}

const FAILURE_PHRASES = [
  "authentication is required",
  "auth storage is unavailable",
  "ServerError|",
  "error deserializing server function",
  "rate-limit operation failed",
  "Request origin rejected",
];

async function auditRoute(page, route) {
  const apiErrors = [];
  const consoleErrors = [];
  const onResponse = (res) => {
    const u = res.url();
    if (u.includes("/api/ui/") && res.status() >= 400) {
      apiErrors.push({
        url: u.replace(baseUrl, ""),
        status: res.status(),
      });
    }
  };
  const onConsole = (msg) => {
    if (msg.type() === "error") {
      consoleErrors.push(msg.text().slice(0, 200));
    }
  };
  page.on("response", onResponse);
  page.on("console", onConsole);
  let finalUrl = "";
  let h1 = "";
  let bodyHit = [];
  try {
    await page.goto(url(route.path), {
      waitUntil: "domcontentloaded",
      timeout: 30_000,
    });
    await page.waitForLoadState("networkidle", { timeout: 8_000 }).catch(() => {});
    await page.waitForTimeout(400);
    finalUrl = page.url();
    h1 = (await page.locator("h1").first().textContent().catch(() => ""))?.trim() || "";
    const body = (await page.locator("body").innerText().catch(() => "")).toLowerCase();
    bodyHit = FAILURE_PHRASES.filter((p) => body.includes(p.toLowerCase()));
  } catch (error) {
    page.off("response", onResponse);
    page.off("console", onConsole);
    return {
      path: route.path,
      error: String(error).slice(0, 240),
      ok: false,
    };
  }
  page.off("response", onResponse);
  page.off("console", onConsole);

  const pathname = new URL(finalUrl).pathname;
  const isAuthRequired = pathname.startsWith("/auth/required");
  const isForbidden =
    pathname.startsWith("/auth/forbidden") ||
    bodyHit.some((b) => b.includes("cannot access"));
  const hardApi = apiErrors.filter((e) => e.status >= 500);
  const clientApi = apiErrors.filter((e) => e.status >= 400 && e.status < 500);

  let ok = hardApi.length === 0 && bodyHit.length === 0;
  if (isAuthRequired && !route.guestOk) ok = false;
  if (isForbidden && route.allowForbidden) {
    // expected for admin as non-sysadmin
    ok = hardApi.length === 0;
  }

  return {
    path: route.path,
    finalPath: pathname,
    h1,
    ok,
    isAuthRequired,
    isForbidden,
    hardApi,
    clientApi,
    consoleErrors: consoleErrors.slice(0, 5),
    bodyHit,
  };
}

async function mutationProbes(page, slug) {
  const results = [];
  async function probe(label, navigate, action) {
    const apiErrors = [];
    const handler = async (res) => {
      if (res.url().includes("/api/ui/") && res.status() >= 400) {
        let body = "";
        try {
          body = await res.text();
        } catch {
          /* ignore */
        }
        apiErrors.push({
          url: res.url().replace(baseUrl, ""),
          status: res.status(),
          body: body.slice(0, 160),
        });
      }
    };
    page.on("response", handler);
    try {
      await page.goto(url(navigate), { waitUntil: "domcontentloaded" });
      await page.waitForTimeout(800);
      await action(page);
      await page.waitForTimeout(1500);
    } catch (error) {
      apiErrors.push({ actionError: String(error).slice(0, 200) });
    }
    page.off("response", handler);
    results.push({
      label,
      ok: !apiErrors.some((e) => (e.status || 0) >= 500),
      apiErrors,
    });
  }

  await probe("vault-seed-demos", `/org/${slug}/vault`, async (p) => {
    const btn = p.getByRole("button", { name: /Load demo connectors/i });
    if (await btn.count()) await btn.click();
  });
  await probe("vault-add-secret", `/org/${slug}/vault`, async (p) => {
    const add = p.getByRole("button", { name: /Add secret/i });
    if (!(await add.count())) return;
    await add.click();
    await p.waitForTimeout(300);
    const key = p.getByRole("textbox", { name: /^Key$/i });
    if (await key.count()) {
      await key.fill(`AUDIT_KEY_${Date.now()}`);
      await p.getByRole("textbox", { name: /^Label$/i }).fill("Audit label");
      await p.getByRole("textbox", { name: /Value/i }).fill("secret-value");
      await p.getByRole("button", { name: /Store secret/i }).click();
    }
  });
  await probe("settings-general-rename", `/org/${slug}/settings/general`, async (p) => {
    const input = p.locator('input[type="text"]').first();
    if (!(await input.count())) return;
    const current = await input.inputValue();
    await input.fill(`${current} `.trimEnd() + "x");
    const save = p.getByRole("button", { name: /save/i }).first();
    if (await save.count()) await save.click();
  });

  return results;
}

async function main() {
  console.log(`Audit base: ${baseUrl}`);
  console.log(`Account: ${email}`);
  const sessionId = await ensureAccount();
  console.log(`Session: ${sessionId}`);
  // Promote AAL for wasi-auth SQL mutations in case promote-on-login not active
  try {
    const { spawnSync } = await import("node:child_process");
    spawnSync(
      "psql",
      [
        "-h",
        "127.0.0.1",
        "-p",
        "54329",
        "-U",
        "wasi_auth",
        "-d",
        "wasi_auth",
        "-c",
        `UPDATE auth_sessions SET assurance='aal2' WHERE session_id='${sessionId}'`,
      ],
      {
        env: { ...process.env, PGPASSWORD: "wasi_auth_dev" },
        stdio: "ignore",
      },
    );
  } catch {
    /* optional */
  }
  const { slug } = await ensureOrg(sessionId);
  console.log(`Workspace slug: ${slug}`);

  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({
    baseURL: baseUrl,
    viewport: { width: 1280, height: 800 },
  });
  await context.addCookies([
    {
      name: "wasi_auth_dev_session",
      value: sessionId,
      url: baseUrl,
      httpOnly: true,
      sameSite: "Lax",
    },
  ]);
  const page = await context.newPage();

  const routes = routesForSlug(slug);
  const pageResults = [];
  for (const route of routes) {
    const result = await auditRoute(page, route);
    pageResults.push(result);
    const mark = result.ok ? "OK " : "FAIL";
    console.log(
      `${mark} ${route.path.padEnd(42)} → ${result.finalPath || "?"} h1="${(result.h1 || "").slice(0, 40)}"`,
    );
    if (!result.ok) {
      if (result.hardApi?.length) console.log("     hardApi", result.hardApi);
      if (result.clientApi?.length) console.log("     clientApi", result.clientApi);
      if (result.bodyHit?.length) console.log("     bodyHit", result.bodyHit);
      if (result.consoleErrors?.length)
        console.log("     console", result.consoleErrors);
      if (result.error) console.log("     error", result.error);
    }
  }

  console.log("\n--- Mutation probes ---");
  const mutations = await mutationProbes(page, slug);
  for (const m of mutations) {
    console.log(`${m.ok ? "OK " : "FAIL"} ${m.label}`, m.apiErrors?.length ? m.apiErrors : "");
  }

  await browser.close();

  const failedPages = pageResults.filter((r) => !r.ok);
  const failedMut = mutations.filter((m) => !m.ok);
  console.log("\n=== SUMMARY ===");
  console.log(`pages: ${pageResults.length}, failed: ${failedPages.length}`);
  console.log(`mutations: ${mutations.length}, failed: ${failedMut.length}`);
  if (failedPages.length) {
    console.log(
      "failed pages:",
      failedPages.map((f) => f.path).join(", "),
    );
  }
  if (failedMut.length) {
    console.log(
      "failed mutations:",
      failedMut.map((f) => f.label).join(", "),
    );
  }
  process.exit(failedPages.length || failedMut.length ? 1 : 0);
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
