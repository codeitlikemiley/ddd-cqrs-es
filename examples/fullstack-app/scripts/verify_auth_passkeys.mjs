#!/usr/bin/env node

const baseUrl =
  process.env.BASE_URL ||
  process.env.AUTH_PUBLIC_BASE_URL ||
  "http://localhost:3008";

async function loadPlaywright() {
  try {
    return await import("playwright");
  } catch (error) {
    console.error("Playwright is required for passkey browser smoke checks.");
    console.error(
      "Run with: npx -y -p playwright node examples/fullstack-app/scripts/verify_auth_passkeys.mjs",
    );
    throw error;
  }
}

function url(path) {
  return new URL(path, baseUrl).toString();
}

function assertLocalhostOrigin() {
  const parsed = new URL(baseUrl);
  if (parsed.hostname !== "localhost") {
    throw new Error(
      `Passkey smoke must use hostname "localhost" so WebAuthn rpId matches (set listen=localhost:PORT or AUTH_PASSKEY_RP_ID); got ${baseUrl}`,
    );
  }
}

async function assertPasskeyCapability() {
  const response = await fetch(url("/api/auth/capabilities"));
  if (!response.ok) {
    throw new Error(
      `Capabilities request failed with ${response.status}: ${await response.text()}`,
    );
  }
  const capabilities = await response.json();
  if (capabilities.passkeys_enabled !== true) {
    throw new Error(
      "Passkey smoke requires AUTH_ENABLE_PASSKEYS=true on the running server.",
    );
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
        `verification mail capture failed: ${response.status} ${await response.text()}`,
      );
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Mail worker did not deliver ${kind}; last status ${lastStatus}`);
}

async function createVerifiedSession(email) {
  const register = await fetch(url("/api/auth/password/register"), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      email,
      password: "passkey-browser-correct-123",
      redirect_url: "/dashboard",
    }),
  });
  if (!register.ok) {
    throw new Error(`password registration failed: ${register.status} ${await register.text()}`);
  }
  const pending = await register.json();
  if (pending.authenticated !== false) {
    throw new Error("password registration did not create a pending account");
  }
  const message = await capturedMail(email, "email-verification");
  const token = new URL(message.body_text, baseUrl).searchParams.get("token");
  if (!token) throw new Error("verification mail did not contain a token");
  const verify = await fetch(url("/api/auth/email/verify"), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ token, redirect_url: "/dashboard" }),
  });
  if (!verify.ok) {
    throw new Error(`email verification failed: ${verify.status} ${await verify.text()}`);
  }
  const session = await verify.json();
  if (!session.session_id) throw new Error("verification did not return a session");
  return session.session_id;
}

async function attachVirtualAuthenticator(context, page) {
  const client = await context.newCDPSession(page);
  await client.send("WebAuthn.enable");
  await client.send("WebAuthn.addVirtualAuthenticator", {
    options: {
      protocol: "ctap2",
      transport: "internal",
      hasResidentKey: true,
      hasUserVerification: true,
      isUserVerified: true,
      automaticPresenceSimulation: true,
    },
  });
}

async function runPasskeyRegistration(page, email) {
  return page.evaluate(async (email) => {
    function b64urlToBuffer(value) {
      const padding = "=".repeat((4 - (value.length % 4)) % 4);
      const base64 = (value + padding).replace(/-/g, "+").replace(/_/g, "/");
      const binary = atob(base64);
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

    const csrfResponse = await fetch("/api/auth/csrf");
    if (!csrfResponse.ok) {
      throw new Error(`CSRF token request failed: ${csrfResponse.status}`);
    }
    const csrf = (await csrfResponse.json()).token;
    const start = await fetch("/api/auth/passkeys/register/options", {
      method: "POST",
      headers: { "content-type": "application/json", "x-csrf-token": csrf },
      body: JSON.stringify({ email, redirect_url: "/dashboard" }),
    });
    if (!start.ok) {
      throw new Error(`registration options failed: ${start.status} ${await start.text()}`);
    }
    const optionsResponse = await start.json();
    const publicKey = JSON.parse(optionsResponse.public_key_options_json);
    publicKey.challenge = b64urlToBuffer(publicKey.challenge);
    publicKey.user.id = b64urlToBuffer(publicKey.user.id);
    publicKey.excludeCredentials = Array.isArray(publicKey.excludeCredentials)
      ? publicKey.excludeCredentials.map((descriptor) => ({
          ...descriptor,
          id: b64urlToBuffer(descriptor.id),
        }))
      : publicKey.excludeCredentials;

    const credential = await navigator.credentials.create({ publicKey });
    if (!credential) {
      throw new Error("virtual authenticator did not create a credential");
    }

    const credentialJson = JSON.stringify({
      id: bufferToB64url(credential.rawId),
      transports: credential.response.getTransports
        ? credential.response.getTransports()
        : [],
      attestationObject: bufferToB64url(credential.response.attestationObject),
      clientDataJSON: bufferToB64url(credential.response.clientDataJSON),
    });

    const verify = await fetch("/api/auth/passkeys/register/verify", {
      method: "POST",
      headers: { "content-type": "application/json", "x-csrf-token": csrf },
      body: JSON.stringify({
        challenge_id: optionsResponse.challenge_id,
        credential_json: credentialJson,
        redirect_url: "/dashboard",
      }),
    });
    if (!verify.ok) {
      throw new Error(`registration verify failed: ${verify.status} ${await verify.text()}`);
    }
    return verify.json();
  }, email);
}

async function runPasskeyLogin(page, email) {
  return page.evaluate(async (email) => {
    function b64urlToBuffer(value) {
      const padding = "=".repeat((4 - (value.length % 4)) % 4);
      const base64 = (value + padding).replace(/-/g, "+").replace(/_/g, "/");
      const binary = atob(base64);
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

    const start = await fetch("/api/auth/passkeys/login/options", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ email, redirect_url: "/dashboard" }),
    });
    if (!start.ok) {
      throw new Error(`login options failed: ${start.status} ${await start.text()}`);
    }
    const optionsResponse = await start.json();
    const publicKey = JSON.parse(optionsResponse.public_key_options_json);
    publicKey.challenge = b64urlToBuffer(publicKey.challenge);
    publicKey.allowCredentials = Array.isArray(publicKey.allowCredentials)
      ? publicKey.allowCredentials.map((descriptor) => ({
          ...descriptor,
          id: b64urlToBuffer(descriptor.id),
        }))
      : publicKey.allowCredentials;

    const credential = await navigator.credentials.get({ publicKey });
    if (!credential) {
      throw new Error("virtual authenticator did not return a credential");
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

    const verify = await fetch("/api/auth/passkeys/login/verify", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        challenge_id: optionsResponse.challenge_id,
        credential_json: JSON.stringify(response),
        redirect_url: "/dashboard",
      }),
    });
    if (!verify.ok) {
      throw new Error(`login verify failed: ${verify.status} ${await verify.text()}`);
    }
    return verify.json();
  }, email);
}

assertLocalhostOrigin();
await assertPasskeyCapability();

const { chromium } = await loadPlaywright();
const browser = await chromium.launch({ headless: true });

try {
  const context = await browser.newContext({ baseURL: baseUrl });
  const page = await context.newPage();
  await attachVirtualAuthenticator(context, page);
  await page.goto(url("/login"), { waitUntil: "domcontentloaded" });

  const email = `passkey-browser-${Date.now()}@example.test`;
  const unauthenticatedRegistration = await fetch(
    url("/api/auth/passkeys/register/options"),
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ email, redirect_url: "/dashboard" }),
    },
  );
  if (unauthenticatedRegistration.ok) {
    throw new Error("unauthenticated passkey registration unexpectedly succeeded");
  }
  const sessionId = await createVerifiedSession(email);
  await context.addCookies([
    {
      name: "wasi_auth_dev_session",
      value: sessionId,
      domain: new URL(baseUrl).hostname,
      path: "/",
      httpOnly: true,
      sameSite: "Lax",
    },
  ]);
  const registration = await runPasskeyRegistration(page, email);
  if (!registration.authenticated || registration.redirect_url !== "/dashboard") {
    throw new Error(`unexpected registration response: ${JSON.stringify(registration)}`);
  }

  const login = await runPasskeyLogin(page, email);
  if (!login.authenticated || login.redirect_url !== "/dashboard") {
    throw new Error(`unexpected login response: ${JSON.stringify(login)}`);
  }

  console.log("fullstack-app passkey browser smoke: passed");
} finally {
  await browser.close();
}
