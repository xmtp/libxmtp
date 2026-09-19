import { expect, test } from "@playwright/test";

test("homepage layout and agent tabs work with keyboard input", async ({
  page,
}) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { level: 1 })).toHaveCount(1);
  const pair = page.getByRole("tab", {
    name: "Connect two agents",
    exact: true,
  });
  const group = page.getByRole("tab", {
    name: "Agent group chat",
    exact: true,
  });
  await pair.focus();
  await pair.press("ArrowRight");
  await expect(group).toBeFocused();
  await expect(group).toHaveAttribute("aria-selected", "true");
  await expect(page.locator("#agent-panel-two")).toBeHidden();
  await page.getByRole("button", { name: "Replay example" }).click();
  await page.getByRole("button", { name: "Replay example" }).click();
  await group.press("Home");
  await expect(pair).toBeFocused();
  await pair.press("End");
  await expect(page.locator("example-transcript .message:visible")).toHaveCount(
    6,
  );
  for (const width of [320, 390, 768, 1440]) {
    await page.setViewportSize({ width, height: 900 });
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
  }
});

test("copy uses the same text as the preview and validates backend URLs", async ({
  page,
}) => {
  const backendRequests = [];
  page.on("request", (request) => {
    if (new URL(request.url()).hostname === "backend.example.com") {
      backendRequests.push(request.url());
    }
  });
  await page.addInitScript(() => {
    Object.defineProperty(navigator, "clipboard", {
      value: {
        writeText: async (text) => {
          window.copiedPrompt = text;
        },
      },
    });
  });
  await page.goto("/");
  const integration = page.locator('home-prompt[data-prompt="integration"]');
  await integration.locator(".backend summary").click();
  const input = integration.getByLabel("Backend URL", { exact: true });
  await input.fill("https://user:secret@example.com");
  await expect(input).toHaveAttribute("aria-invalid", "true");
  await expect(integration.locator("button")).toBeDisabled();
  await input.fill("https://backend.example.com");
  await expect(input).not.toHaveAttribute("aria-invalid");
  for (const id of ["integration", "inbox", "group"]) {
    if (id === "group")
      await page
        .getByRole("tab", { name: "Agent group chat", exact: true })
        .click();
    const prompt = page.locator(`home-prompt[data-prompt="${id}"]`);
    await prompt.locator("button.copy").click();
    await expect(prompt.getByRole("status")).toContainText("Prompt copied");
    expect(await page.evaluate(() => window.copiedPrompt)).toBe(
      await prompt.locator(".full-prompt pre").textContent(),
    );
  }
  expect(backendRequests).toEqual([]);
});

test("clipboard denial opens a manual copy fallback", async ({ page }) => {
  await page.addInitScript(() => {
    Object.defineProperty(navigator, "clipboard", {
      value: {
        writeText: async () => {
          throw new Error("Denied");
        },
      },
    });
  });
  await page.goto("/");
  const prompt = page.locator('home-prompt[data-prompt="integration"]');
  await prompt.locator("button").click();
  await expect(prompt.locator("pre")).toBeVisible();
  await expect(prompt.locator("pre")).toBeFocused();
  await expect(prompt.getByRole("status")).toContainText("Select and copy");
});

test("reduced motion keeps the full transcript visible", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/");
  await page
    .getByRole("tab", { name: "Agent group chat", exact: true })
    .click();
  await page.getByRole("button", { name: "Replay example" }).click();
  await expect(page.locator("example-transcript .message:visible")).toHaveCount(
    6,
  );
});

test("content and prompts remain available without JavaScript", async ({
  browser,
  baseURL,
}) => {
  const context = await browser.newContext({
    javaScriptEnabled: false,
    baseURL,
  });
  const page = await context.newPage();
  await page.goto("/");
  await expect(page.locator("#agent-panel-two")).toBeVisible();
  await expect(page.locator("#agent-panel-group")).toBeVisible();
  await expect(page.locator("home-prompt .copy:visible")).toHaveCount(0);
  const prompt = page.locator('home-prompt[data-prompt="integration"]');
  await prompt.locator(".full-prompt summary").click();
  await expect(prompt.locator("pre")).toBeVisible();
  await context.close();
});

test("guide pages do not load homepage fonts or controls", async ({ page }) => {
  const homeAssets = [];
  page.on("request", (request) => {
    if (request.url().includes("/home/")) homeAssets.push(request.url());
  });
  await page.goto("/get-started/quickstart/");
  await expect(
    page.getByRole("button", { name: /search/i }).first(),
  ).toBeVisible();
  await expect(page.locator(".home-header")).toHaveCount(0);
  await expect(page.locator("home-agent-examples")).toHaveCount(0);
  expect(homeAssets).toEqual([]);
});

test("the light homepage preserves the saved dark docs theme", async ({
  page,
}) => {
  await page.goto("/get-started/quickstart/");
  await page
    .locator("starlight-theme-select select")
    .first()
    .selectOption("dark", { force: true });
  await page.goto("/");
  await expect(page.locator("body")).toHaveCSS(
    "background-color",
    "rgb(255, 255, 255)",
  );
  expect(
    await page.evaluate(() => localStorage.getItem("starlight-theme")),
  ).toBe("dark");
  await page
    .locator(".home-header")
    .getByRole("link", { name: "Docs", exact: true })
    .click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
});

test("homepage content fits the viewport at 200 percent browser zoom", async ({
  browser,
  baseURL,
}) => {
  // A 1280 x 900 viewport at 200% zoom has 640 x 450 CSS pixels.
  const context = await browser.newContext({
    baseURL,
    viewport: { width: 640, height: 450 },
    deviceScaleFactor: 2,
  });
  const page = await context.newPage();
  await page.goto("/");
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page
    .getByRole("tab", { name: "Agent group chat", exact: true })
    .click();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await context.close();
});
