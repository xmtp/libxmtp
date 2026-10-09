import { expect, test } from "@playwright/test";

test("homepage uses the approved content order, security links, and local agent avatars", async ({
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

  await expect(
    page.getByRole("link", { name: "Read how we built it" }),
  ).toHaveAttribute(
    "href",
    "https://blog.xmtp.org/xmtp-and-the-future-of-privacy-in-a-quantum-world/",
  );
  await expect(page.locator("#agents .agent-heading h2")).toHaveText(
    "The open, secure agent network.",
  );
  await expect(page.locator("#consent-preferences h2")).toHaveText(
    "Spam protection, built-in",
  );
  await expect(
    page.getByRole("link", { name: "Explore consent" }),
  ).toHaveAttribute("href", "/sdk/consent/");
  expect(
    await page
      .locator(".home-content > section")
      .evaluateAll((sections) => sections.map((section) => section.id)),
  ).toEqual([
    "",
    "sdks",
    "agents",
    "start",
    "security",
    "consent-preferences",
    "start-building-today",
  ]);
  await expect(page.getByRole("tab").first()).toHaveText(
    "Create an agent group",
  );
  await expect(page.getByRole("tab").first()).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(page.locator("#agent-panel-two")).toBeHidden();
  await expect(page.locator("#agent-panel-group")).toBeVisible();
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

test("homepage links open the install, security, and client guides", async ({
  page,
}) => {
  for (const [scope, name, path, heading] of [
    [".home-header", "SDKs", "/get-started/install/", "Install the XMTP SDK"],
    [".home-header", "Security", "/protocol/security/", "Messaging security"],
    [
      ".home-hero",
      "Build with your coding assistant",
      "/sdk/client/",
      "Client",
    ],
  ]) {
    await page.goto("/");
    const link = page.locator(scope).getByRole("link", { name });
    await expect(link).toHaveAttribute("href", path);
    await link.click();
    await expect(page).toHaveURL(new RegExp(`${path.replace(/\/$/, "")}/?$`));
    await expect(page.getByRole("heading", { level: 1 })).toHaveText(heading);
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
  await group.press("End");
  await expect(pair).toBeFocused();
  await expect(page.locator("#agent-panel-group")).toBeHidden();
  await pair.press("Home");
  await expect(group).toBeFocused();
  await expect(page.locator("example-transcript .message:visible")).toHaveCount(
    6,
  );
  await page.evaluate(() => document.fonts.ready);
  const network = page.locator(".network");
  for (const width of [320, 390, 768, 1024, 1440]) {
    await page.setViewportSize({ width, height: 900 });
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
    const issues = await network.evaluate((element) => {
      const panel = element.getBoundingClientRect();
      const nodes = [...element.querySelectorAll(".group-agent, .center")];
      const failures = [];
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
    expect(issues, `${width}px agent diagram`).toEqual([]);
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
    if (id !== "integration")
      await page
        .getByRole("tab", {
          name: id === "group" ? "Create an agent group" : "Connect two agents",
          exact: true,
        })
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
