import { expect, test } from "@playwright/test";

test("homepage uses the approved hero, security link, and local agent avatars", async ({
  page,
}) => {
  const prototypeRequests = [];
  page.on("request", (request) => {
    if (new URL(request.url()).hostname.endsWith("shanemacsora.chatgpt.site")) {
      prototypeRequests.push(request.url());
    }
  });
  await page.goto("/");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText(
    "Build secure messaging for people and agents.",
  );
  await expect(page.locator(".home-hero .home-label")).toHaveText(
    "OPEN SOURCE · END-TO-END ENCRYPTED · QUANTUM-RESISTANT",
  );
  await expect(
    page.getByRole("link", { name: /How quantum resistance works/ }),
  ).toHaveAttribute("href", "/protocol/security/#quantum-resistance");

  await page
    .getByRole("tab", { name: "Create an agent group", exact: true })
    .click();
  const network = page.locator(".network");
  await network.scrollIntoViewIfNeeded();
  const avatars = network.locator(".badge img");
  await expect(avatars).toHaveCount(6);
  const avatarData = await avatars.evaluateAll((images) =>
    images.map((image) => ({ alt: image.alt, src: image.getAttribute("src") })),
  );
  expect(avatarData.map(({ alt }) => alt)).toEqual([
    "Doc",
    "Instinct",
    "Muse",
    "Codex",
    "Claude",
    "Grokbot",
  ]);
  expect(avatarData.every(({ src }) => src?.startsWith("/_astro/"))).toBe(true);
  expect(prototypeRequests).toEqual([]);
});

test("homepage header opens the SDK guide and security docs", async ({
  page,
}) => {
  for (const [name, path, heading] of [
    ["SDKs", "/sdk/client/", "Client"],
    ["Security", "/protocol/security/", "Messaging security"],
  ]) {
    await page.goto("/");
    const link = page
      .getByRole("navigation", { name: "Main navigation" })
      .getByRole("link", { name, exact: true });
    await expect(link).toHaveAttribute("href", path);
    await link.click();
    await expect(page).toHaveURL(path);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText(heading);
  }
});

test("group avatars fit their badges and diagram nodes do not overlap", async ({
  page,
}) => {
  await page.goto("/");
  await page
    .getByRole("tab", { name: "Create an agent group", exact: true })
    .click();
  await page.evaluate(() => document.fonts.ready);
  for (const width of [320, 390, 768, 1024, 1440]) {
    await page.setViewportSize({ width, height: 900 });
    const issues = await page.locator(".network").evaluate((network) => {
      const failures = [];
      const panel = network.getBoundingClientRect();
      for (const badge of network.querySelectorAll(".badge")) {
        const box = badge.getBoundingClientRect();
        const avatar = badge.querySelector("img");
        const image = avatar.getBoundingClientRect();
        if (!avatar.alt) failures.push("Avatar has no accessible name");
        if (
          image.left < box.left ||
          image.right > box.right ||
          image.top < box.top ||
          image.bottom > box.bottom
        ) {
          failures.push(`Avatar exceeds badge: ${avatar.alt}`);
        }
      }
      const nodes = [...network.querySelectorAll(".group-agent, .center")];
      nodes.forEach((node, index) => {
        const box = node.getBoundingClientRect();
        if (box.left < panel.left || box.right > panel.right) {
          failures.push(`Node exceeds panel: ${node.textContent}`);
        }
        for (const other of nodes.slice(index + 1)) {
          const next = other.getBoundingClientRect();
          if (
            box.left < next.right &&
            box.right > next.left &&
            box.top < next.bottom &&
            box.bottom > next.top
          ) {
            failures.push(
              `Nodes overlap: ${node.textContent} / ${other.textContent}`,
            );
          }
        }
      });
      return failures;
    });
    expect(issues, `${width}px diagram`).toEqual([]);
  }
});

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
    name: "Create an agent group",
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
        .getByRole("tab", { name: "Create an agent group", exact: true })
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
    .getByRole("tab", { name: "Create an agent group", exact: true })
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

test("the light homepage leaves docs on the automatic system theme", async ({
  page,
}) => {
  await page.goto("/get-started/quickstart/");
  await page.emulateMedia({ colorScheme: "dark" });
  await page.goto("/");
  await expect(page.locator("body")).toHaveCSS(
    "background-color",
    "rgb(255, 255, 255)",
  );
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
    .getByRole("tab", { name: "Create an agent group", exact: true })
    .click();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await context.close();
});
