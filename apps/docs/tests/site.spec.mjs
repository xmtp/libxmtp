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
  await page.goto("/");
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

test("the search UI uses the guide ranking", async ({ page }) => {
  await page.goto("/");
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
  expect(logo.height).toBeLessThanOrEqual(40);
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth),
  ).toBeLessThanOrEqual(viewport.width);
});

test("native references load their content and styles", async ({ page }) => {
  for (const path of [
    "/rust/",
    "/reference/kotlin/",
    "/reference/swift/documentation/xmtpios/",
  ]) {
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
