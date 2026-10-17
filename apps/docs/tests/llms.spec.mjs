import { expect, test } from "@playwright/test";

test("website diagrams stay visible and export source stays hidden", async ({
  page,
}) => {
  for (const route of ["/specs/api-backend-api/", "/protocol/cursors/"]) {
    await page.goto(route);
    const image = page.locator(".llms-rendered-diagram img").first();
    await expect(image).toBeVisible();
    await expect
      .poll(() => image.evaluate((node) => node.naturalWidth))
      .toBeGreaterThan(0);
    const source = page.locator(".llms-diagram-source").first();
    await expect(source).toBeAttached();
    await expect(source).toBeHidden();
    expect(await source.textContent()).toMatch(/flowchart|sequenceDiagram/);
  }
});
