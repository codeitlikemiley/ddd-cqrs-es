#!/usr/bin/env node

const baseUrl = process.env.BASE_URL || "http://localhost:3008";

async function loadPlaywright() {
  try {
    return await import("playwright");
  } catch (error) {
    console.error("Playwright is required for passkey browser smoke checks.");
    console.error(
      "Run with: npx -y -p playwright node examples/auth-stack/scripts/verify_auth_passkeys.mjs",
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
      `Passkey smoke must use http://localhost so the browser origin matches AUTH_PASSKEY_RP_ID=localhost; got ${baseUrl}`,
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

    const start = await fetch("/api/auth/passkeys/register/options", {
      method: "POST",
      headers: { "content-type": "application/json" },
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
      headers: { "content-type": "application/json" },
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
  const registration = await runPasskeyRegistration(page, email);
  if (!registration.authenticated || registration.redirect_url !== "/dashboard") {
    throw new Error(`unexpected registration response: ${JSON.stringify(registration)}`);
  }

  const login = await runPasskeyLogin(page, email);
  if (!login.authenticated || login.redirect_url !== "/dashboard") {
    throw new Error(`unexpected login response: ${JSON.stringify(login)}`);
  }

  console.log("auth-stack passkey browser smoke: passed");
} finally {
  await browser.close();
}
