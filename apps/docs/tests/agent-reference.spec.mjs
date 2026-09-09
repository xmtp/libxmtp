import { expect, test } from "@playwright/test";

test("Agent API reference renders inside the site", async ({
  page,
}, testInfo) => {
  await page.goto("/reference/agent-sdk/classes/agent/");
  await expect(page.locator("h1")).toHaveText("Agent");
  await expect(
    page.getByRole("heading", { name: "Methods", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("navigation", { name: "Sections" }),
  ).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath("agent-reference.png") });
});
