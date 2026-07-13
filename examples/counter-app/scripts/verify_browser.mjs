#!/usr/bin/env node

const baseUrl = process.env.BASE_URL || "http://127.0.0.1:3000";

async function loadPlaywright() {
  try {
    return await import("playwright");
  } catch (error) {
    console.error("Playwright is required for the counter browser smoke check.");
    console.error("Run npm install in examples/counter-app first.");
    throw error;
  }
}

const { chromium } = await loadPlaywright();
const browser = await chromium.launch({ headless: true });
const diagnostics = [];

try {
  const context = await browser.newContext({ viewport: { width: 1280, height: 720 } });
  const page = await context.newPage();
  const requestedWasm = new Set();
  const pendingWasm = new Set();

  page.on("request", (request) => {
    const pathname = new URL(request.url()).pathname;
    if (pathname.endsWith(".wasm")) {
      requestedWasm.add(pathname);
      pendingWasm.add(pathname);
    }
  });
  page.on("requestfinished", (request) => {
    pendingWasm.delete(new URL(request.url()).pathname);
  });
  page.on("requestfailed", (request) => {
    pendingWasm.delete(new URL(request.url()).pathname);
    const reason = request.failure()?.errorText || "unknown error";
    if (reason !== "net::ERR_ABORTED") {
      diagnostics.push(`request failed: ${request.method()} ${request.url()} ${reason}`);
    }
  });
  page.on("console", async (message) => {
    if (["error", "warning"].includes(message.type())) {
      const renderedArgs = await Promise.all(
        message.args().map(async (argument) => {
          try {
            return await argument.evaluate((value) => {
              if (value && typeof value === "object" && "nodeType" in value) {
                return value.outerHTML || `node(type=${value.nodeType}, text=${value.textContent || ""})`;
              }
              return String(value);
            });
          } catch {
            return argument.toString();
          }
        }),
      );
      diagnostics.push(
        `console ${message.type()}: ${renderedArgs.length > 0 ? renderedArgs.join(" ") : message.text()}`,
      );
    }
  });
  page.on("pageerror", (error) => diagnostics.push(`page error: ${error.message}`));

  await page.goto(baseUrl, { waitUntil: "domcontentloaded" });
  await page.getByRole("heading", { name: "CQRS Counter" }).waitFor();

  const components = await page.locator("leptos-island").evaluateAll((islands) =>
    islands.map((island) => island.getAttribute("data-component") || ""),
  );
  if (components.length !== 1 || !components[0].startsWith("CounterPanel_")) {
    throw new Error(`Expected only the lazy CounterPanel island, got ${JSON.stringify(components)}`);
  }

  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline) {
    const splitRequested = [...requestedWasm].some((pathname) =>
      pathname.split("/").pop()?.startsWith("split_counter_panel_loader_"),
    );
    if (splitRequested && pendingWasm.size === 0) break;
    await page.waitForTimeout(50);
  }
  if (
    ![...requestedWasm].some((pathname) =>
      pathname.split("/").pop()?.startsWith("split_counter_panel_loader_"),
    ) ||
    pendingWasm.size !== 0
  ) {
    throw new Error(
      `Lazy CounterPanel WASM did not finish loading: ${JSON.stringify([...pendingWasm])}`,
    );
  }

  const counter = page.getByTestId("counter-value");
  await counter.waitFor();
  await page.waitForFunction(
    () => document.querySelector('[data-testid="counter-value"]')?.textContent?.trim() !== "...",
  );
  const before = Number.parseInt((await counter.textContent()).trim(), 10);
  await page.getByRole("button", { name: /Increment/u }).click();
  await page.waitForFunction(
    (previous) =>
      Number.parseInt(
        document.querySelector('[data-testid="counter-value"]')?.textContent?.trim() || "",
        10,
      ) === previous + 1,
    before,
  );

  await page.waitForTimeout(100);
  if (diagnostics.length > 0) {
    throw new Error(`Unexpected browser diagnostics:\n${diagnostics.join("\n")}`);
  }
  console.log("counter browser hydration and lazy-island smoke: passed");
  await context.close();
} catch (error) {
  const detail = diagnostics.length > 0 ? `\n${diagnostics.join("\n")}` : "";
  throw new Error(`${error.message}${detail}`, { cause: error });
} finally {
  await browser.close();
}
