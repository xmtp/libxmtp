import { expect, test } from "@playwright/test";
import { SEARCH_CASES } from "../scripts/check-search.mjs";
import { searchRanking } from "../scripts/search-config.mjs";

test("navigation works and the page has the main landmarks", async ({
  page,
}) => {
  await page.goto("/");
  await expect(page.locator("header")).toBeVisible();
  await expect(page.locator("main")).toBeVisible();
  await expect(page.locator("footer")).toBeVisible();
  const firstInternalLink = page.locator('main a[href^="/"]').first();
  await expect(firstInternalLink).toBeVisible();
  const destination = await firstInternalLink.getAttribute("href");
  await firstInternalLink.click();
  await expect(page).toHaveURL(
    new RegExp(destination.replace(/[.*+?^$()|[\]\\]/g, "\\$&")),
  );
});

test("search opens from the button and keyboard shortcut", async ({ page }) => {
  await page.goto("/get-started/quickstart/");
  const searchButton = page.getByRole("button", { name: /search/i }).first();
  await searchButton.click();
  const search = page
    .getByRole("searchbox")
    .or(page.getByPlaceholder(/search/i))
    .first();
  await expect(search).toBeVisible();
  await page.keyboard.press("Escape");
  await page.keyboard.press(
    process.platform === "darwin" ? "Meta+KeyK" : "Control+KeyK",
  );
  await expect(search).toBeVisible();
});

test("all search cases rank the expected page first", async ({ page }) => {
  await page.goto("/");
  for (const [query, expected] of SEARCH_CASES) {
    const top = await page.evaluate(
      async ({ value, ranking }) => {
        const pagefind = await import("/pagefind/pagefind.js");
        await pagefind.options({ ranking });
        await pagefind.init();
        const response = await pagefind.search(value);
        return response.results[0]
          ? (await response.results[0].data()).url
          : "(none)";
      },
      { value: query, ranking: searchRanking },
    );
    expect.soft(top, query).toBe(expected);
  }
});

test("specs stay searchable without the guide title boost", async ({
  page,
}) => {
  await page.goto("/specs/join-joining-groups/");
  const title = page.locator("h1#_top");
  await expect(title).toHaveAttribute("data-pagefind-weight", "0.1");
  await expect(title).not.toHaveAttribute("data-pagefind-meta", "guideTitle");
  const urls = await page.evaluate(async (ranking) => {
    const pagefind = await import("/pagefind/pagefind.js");
    await pagefind.options({ ranking });
    await pagefind.init();
    const response = await pagefind.search("WelcomePointerWrapperAlgorithm");
    return Promise.all(
      response.results.map(async (result) => (await result.data()).url),
    );
  }, searchRanking);
  expect(urls).toContain("/specs/join-joining-groups/");
});

test("the search UI uses the guide ranking", async ({ page }) => {
  await page.goto("/get-started/quickstart/");
  await page
    .getByRole("button", { name: /search/i })
    .first()
    .click();
  const input = page.getByRole("textbox", { name: "Search", exact: true });
  for (const [query, expected] of SEARCH_CASES) {
    await input.fill(query);
    await expect(
      page.locator(".pagefind-ui__result-link").first(),
    ).toHaveAttribute("href", expected);
  }
});

test("mobile navigation opens at 390 pixels", async ({ page, viewport }) => {
  await page.goto("/get-started/quickstart/");
  if (viewport.width === 390) {
    const menu = page.getByRole("button", { name: /menu|navigation/i }).first();
    await expect(menu).toBeVisible();
    await menu.click();
    await expect(page.locator("nav").first()).toBeVisible();
  } else {
    await expect(page.locator("nav").first()).toBeVisible();
  }
});

test("SDK tabs synchronize and package manager tabs stay separate", async ({
  page,
}) => {
  await page.goto("/get-started/install/");
  const swiftTab = page.getByRole("tab", { name: "Swift", exact: true });
  await swiftTab.click();
  const packageTab = page.getByRole("tab", { name: /npm|yarn|pnpm/i }).first();
  await expect(packageTab).toHaveAttribute("aria-selected", "true");
  await page.goto("/get-started/quickstart/");
  const swiftTabs = page.getByRole("tab", { name: "Swift", exact: true });
  expect(await swiftTabs.count()).toBeGreaterThanOrEqual(2);
  for (const tab of await swiftTabs.all())
    await expect(tab).toHaveAttribute("aria-selected", "true");
});

test("the docs use the system theme without a selector", async ({ page }) => {
  await page.addInitScript(() =>
    localStorage.setItem("starlight-theme", "dark"),
  );
  await page.emulateMedia({ colorScheme: "light" });
  await page.goto("/get-started/quickstart/");
  await expect(page.locator("starlight-theme-select")).toHaveCount(0);
  await expect(
    page.getByRole("combobox", { name: "Select theme" }),
  ).toHaveCount(0);
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await page.emulateMedia({ colorScheme: "dark" });
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
});

test("the header fits above the page content", async ({ page, viewport }) => {
  await page.goto("/get-started/quickstart/");
  const header = await page.locator("header").boundingBox();
  const sections = await page
    .getByRole("navigation", { name: "Sections" })
    .boundingBox();
  expect(sections.y + sections.height).toBeLessThanOrEqual(
    header.y + header.height + 1,
  );
  const logo = await page.locator(".site-title img:visible").boundingBox();
  const row = await page.locator(".docs-header").boundingBox();
  const search = await page
    .locator("site-search [data-open-modal]")
    .boundingBox();
  const social = page.locator(".docs-header .social");
  const last = (await social.isVisible()) ? await social.boundingBox() : search;
  expect(Math.abs(last.x + last.width - row.x - row.width)).toBeLessThan(2);
  if (await social.isVisible()) {
    expect(search.x + search.width).toBeLessThan(last.x);
  }
  expect(logo.height).toBeLessThanOrEqual(40);
  await expect(page.locator(".site-title img:visible")).toHaveAttribute(
    "src",
    /xmtp-logo.*\.svg/,
  );
  const dark =
    (await page.locator("html").getAttribute("data-theme")) === "dark";
  await expect(page.locator(".site-title img:visible")).toHaveCSS(
    "filter",
    dark ? "brightness(0) invert(1)" : "brightness(0)",
  );
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth),
  ).toBeLessThanOrEqual(viewport.width);
});

test("native references load their content and styles", async ({ page }) => {
  // The Kotlin and Swift references are built on push only; a pull-request
  // build composes without them. See apps/docs/AGENTS.md.
  const paths =
    process.env.DOCS_SKIP_NATIVE_REFERENCES === "1"
      ? ["/rust/"]
      : [
          "/rust/",
          "/reference/kotlin/",
          "/reference/swift/documentation/xmtpios/",
        ];
  for (const path of paths) {
    await page.goto(path);
    await expect(page.locator("h1").first()).toBeVisible();
    const styles = await page
      .locator('link[rel="stylesheet"]')
      .evaluateAll((links) => links.map((link) => link.href));
    expect(styles.length).toBeGreaterThan(0);
    for (const stylesheet of styles)
      expect((await page.request.get(stylesheet)).ok(), stylesheet).toBe(true);
  }
});
