import { chromium } from "../../../../sdks/browser/node_modules/playwright/index.mjs";

const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage();
  page.on("console", (message) => console.log("browser:", message.text()));
  page.on("pageerror", (error) => console.error("browser error:", error));
  page.on("worker", (worker) => console.log("worker:", worker.url()));
  page.on("response", (response) => {
    if (response.status() >= 400)
      console.error("HTTP", response.status(), response.url());
  });
  page.on("request", (request) => {
    if (request.url().includes(":9150"))
      console.log("backend request:", request.method(), request.url());
  });
  page.on("requestfailed", (request) =>
    console.error("request failed:", request.url(), request.failure()),
  );
  await page.goto(
    "http://127.0.0.1:9419/crates/xmtp_sdk/conformance/ts/browser.html",
  );
  await page.waitForFunction(
    () =>
      document.body.dataset.result === "PASS" ||
      document.body.dataset.result?.startsWith("FAIL"),
    undefined,
    { timeout: 120000 },
  );
  const result = await page.locator("body").getAttribute("data-result");
  if (result !== "PASS") throw new Error(result ?? "worker did not finish");
  console.log("Browser scenarios 1, 2, 7 passed");
} finally {
  await browser.close();
}
