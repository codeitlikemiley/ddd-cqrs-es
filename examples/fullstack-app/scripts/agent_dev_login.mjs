#!/usr/bin/env node
/**
 * Agent / automated browser helper: obtain a session cookie for authenticated UI.
 *
 * Modes
 * ------
 * 1) Existing password account (no mail capture needed):
 *      BROWSER_SMOKE_EMAILS=you@example.test \
 *      BROWSER_SMOKE_PASSWORD='…' \
 *      node scripts/agent_dev_login.mjs
 *
 * 2) Fresh register + captured verification (requires a debug build with the
 *    mail-capture feature, AUTH_MAIL_TRANSPORT=capture, AUTH_DEV_TOOLS=true, and
 *    a loopback AUTH_PUBLIC_BASE_URL):
 *      node scripts/agent_dev_login.mjs --register
 *
 *    Reading captured mail requires `system.user.manage`, so supply a bootstrap
 *    administrator (AUTH_BOOTSTRAP_ADMIN_EMAILS) via AGENT_ADMIN_EMAIL +
 *    AGENT_ADMIN_PASSWORD, or an existing session id via AGENT_ADMIN_SESSION_ID.
 *
 * Output (stdout JSON):
 *   { "email", "session_id", "cookie_header", "storage_state_path?", "action_url?" }
 *
 * Optional:
 *   --storage-state=path.json   Write Playwright storageState for reuse
 *   --print-cookie-only         Print only Cookie header line
 *
 * Example Playwright:
 *   const { session_id } = JSON.parse(await $`node scripts/agent_dev_login.mjs --register`);
 *   await context.addCookies([{ name: 'wasi_auth_dev_session', value: session_id, … }]);
 */
const baseUrl = (
  process.env.BASE_URL ||
  process.env.AUTH_PUBLIC_BASE_URL ||
  "http://127.0.0.1:3008"
).replace(/\/$/, "");

const args = process.argv.slice(2);
const forceRegister = args.includes("--register");
const printCookieOnly = args.includes("--print-cookie-only");
const storageArg = args.find((a) => a.startsWith("--storage-state="));
const storagePath = storageArg
  ? storageArg.slice("--storage-state=".length)
  : process.env.AGENT_STORAGE_STATE || "";

const existingEmail = (process.env.BROWSER_SMOKE_EMAILS || "")
  .split(",")
  .map((e) => e.trim())
  .filter(Boolean)[0];
const existingPassword =
  process.env.BROWSER_SMOKE_PASSWORD || "ChangeMe-Dev-Password-15!";
const registerPassword =
  process.env.BROWSER_SMOKE_PASSWORD || "browser-correct-123";

function url(path) {
  return new URL(path, baseUrl).toString();
}

let adminCookiePromise = null;

// Captured mail exposes anyone's verification link, so the endpoint requires an
// administrator session (`system.user.manage`).
function adminCookie() {
  adminCookiePromise ??= (async () => {
    if (process.env.AGENT_ADMIN_SESSION_ID) {
      return cookieHeader(process.env.AGENT_ADMIN_SESSION_ID);
    }
    const email = process.env.AGENT_ADMIN_EMAIL || existingEmail;
    const password = process.env.AGENT_ADMIN_PASSWORD || existingPassword;
    if (!email) {
      throw new Error(
        "Reading captured mail requires an administrator. Set AGENT_ADMIN_EMAIL + " +
          "AGENT_ADMIN_PASSWORD (an address listed in AUTH_BOOTSTRAP_ADMIN_EMAILS) " +
          "or AGENT_ADMIN_SESSION_ID.",
      );
    }
    const session = await passwordLogin(email, password);
    return cookieHeader(session.sessionId);
  })();
  return adminCookiePromise;
}

async function capturedMail(email, kind) {
  const cookie = await adminCookie();
  let lastStatus = 0;
  for (let attempt = 0; attempt < 50; attempt += 1) {
    const response = await fetch(
      url(
        `/api/auth/dev/mail/latest?recipient=${encodeURIComponent(email)}&kind=${encodeURIComponent(kind)}`,
      ),
      { headers: { cookie } },
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
      `Ensure AUTH_MAIL_TRANSPORT=capture and the outbox worker is running.`,
  );
}

async function registerAndVerify() {
  const email =
    existingEmail ||
    `agent-login-${Date.now()}-${Math.floor(Math.random() * 1e6)}@example.test`;
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
    return {
      email,
      sessionId: body.session_id,
      actionUrl: null,
      password: registerPassword,
    };
  }

  const mail = await capturedMail(email, "email-verification");
  const actionUrl = mail.action_url || null;
  let token = null;
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
    throw new Error(
      "Captured verification mail had no token (check action_url / body_text).",
    );
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
  return {
    email,
    sessionId: verification.session_id,
    actionUrl,
    password: registerPassword,
  };
}

async function passwordLogin(email, password) {
  const response = await fetch(url("/api/auth/password/login"), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      email,
      password,
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
        `Account may need email verification — use --register with capture mail.`,
    );
  }
  return { email, sessionId: body.session_id, actionUrl: null, password };
}

function cookieHeader(sessionId) {
  return `wasi_auth_dev_session=${sessionId}`;
}

async function writeStorageState(sessionId, path) {
  const { writeFile } = await import("node:fs/promises");
  const host = new URL(baseUrl).hostname;
  const state = {
    cookies: [
      {
        name: "wasi_auth_dev_session",
        value: sessionId,
        domain: host,
        path: "/",
        expires: -1,
        httpOnly: true,
        secure: baseUrl.startsWith("https"),
        sameSite: "Lax",
      },
    ],
    origins: [],
  };
  await writeFile(path, JSON.stringify(state, null, 2));
}

async function main() {
  // Priority: --register → always mint a verified session via capture mail.
  // Else existing BROWSER_SMOKE_EMAILS → password login.
  // Else no credentials → register + capture (agent-friendly default).
  const result =
    forceRegister || !existingEmail
      ? await registerAndVerify()
      : await passwordLogin(existingEmail, existingPassword);

  if (storagePath) {
    await writeStorageState(result.sessionId, storagePath);
  }

  if (printCookieOnly) {
    process.stdout.write(`${cookieHeader(result.sessionId)}\n`);
    return;
  }

  const out = {
    base_url: baseUrl,
    email: result.email,
    session_id: result.sessionId,
    cookie_header: cookieHeader(result.sessionId),
    action_url: result.actionUrl,
    storage_state_path: storagePath || null,
    password_hint: forceRegister || !existingEmail ? result.password : undefined,
  };
  process.stdout.write(`${JSON.stringify(out, null, 2)}\n`);
}

main().catch((err) => {
  console.error(String(err?.stack || err));
  process.exit(1);
});
