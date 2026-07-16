#!/usr/bin/env node
/**
 * Optional Playwright smoke: onboarding/workspace vault happy path.
 *
 * Requires an already-running Spin app and BROWSER_SMOKE_EMAILS / password
 * compatible with your local seed (same as verify_auth_pages.mjs).
 *
 *   npx -y -p playwright node scripts/verify_workspace_vault.mjs
 */
const baseUrl =
  process.env.BASE_URL ||
  process.env.AUTH_PUBLIC_BASE_URL ||
  "http://127.0.0.1:3008";
const email = (process.env.BROWSER_SMOKE_EMAILS || "")
  .split(",")
  .map((e) => e.trim())
  .filter(Boolean)[0];
const password =
  process.env.BROWSER_SMOKE_PASSWORD || "ChangeMe-Dev-Password-15!";

async function loadPlaywright() {
  try {
    return await import("playwright");
  } catch {
    console.error("Playwright required: npx -y -p playwright node …");
    process.exit(1);
  }
}

function url(path) {
  return new URL(path, baseUrl).toString();
}

async function main() {
  if (!email) {
    console.log(
      "skip: set BROWSER_SMOKE_EMAILS to run workspace vault browser smoke",
    );
    return;
  }
  const { chromium } = await loadPlaywright();
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  try {
    await page.goto(url("/login"), { waitUntil: "domcontentloaded" });
    await page.waitForTimeout(500);
    // Best-effort login form (islands may differ).
    const emailInput = page.locator('input[type="email"], input[name="email"]').first();
    if (await emailInput.count()) {
      await emailInput.fill(email);
      const pass = page.locator('input[type="password"]').first();
      if (await pass.count()) {
        await pass.fill(password);
      }
      const submit = page.locator('button[type="submit"]').first();
      if (await submit.count()) {
        await submit.click();
        await page.waitForTimeout(2000);
      }
    }

    // Onboarding or dashboard
    const path = new URL(page.url()).pathname;
    if (path.includes("onboarding") || (await page.locator("text=Create your workspace").count())) {
      if (await page.locator("[data-testid='workspace-shell'], .workspace-shell, #workspace-shell").count()) {
        throw new Error("first-workspace onboarding must not render workspace navigation");
      }
      await page.goto(url("/dashboard"), { waitUntil: "domcontentloaded" });
      if (new URL(page.url()).pathname !== "/onboarding/workspace") {
        throw new Error(`dashboard escaped first-workspace onboarding: ${page.url()}`);
      }
      const name = page.locator('input[placeholder*="Acme"], input').first();
      await name.fill("Smoke Vault Org");
      await page.waitForTimeout(200);
      const createBtn = page.locator('button:has-text("Create workspace")').first();
      if (await createBtn.count()) {
        await createBtn.click();
        await page.waitForTimeout(2500);
      }
    }

    // Navigate vault via account redirect
    await page.goto(url("/account/vault"), { waitUntil: "domcontentloaded" });
    await page.waitForTimeout(1500);
    const vaultUrl = page.url();
    if (!vaultUrl.includes("/vault")) {
      throw new Error(`expected vault route, got ${vaultUrl}`);
    }

    // Resources modal should not mention manage vault card - open dashboard
    await page.goto(url("/dashboard"), { waitUntil: "domcontentloaded" });
    await page.waitForTimeout(1000);
    const resources = page.locator('button:has-text("Resources")').first();
    if (await resources.count()) {
      await resources.click();
      await page.waitForTimeout(500);
      const vaultCard = await page.locator("text=Manage vault").count();
      if (vaultCard > 0) {
        throw new Error("Resources modal still exposes Manage vault");
      }
    }

    console.log("verify_workspace_vault: ok");
  } finally {
    await browser.close();
  }
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
