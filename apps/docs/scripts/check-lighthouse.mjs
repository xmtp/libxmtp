import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import lighthouse from "lighthouse";
import { launch } from "chrome-launcher";
import { chromium } from "playwright";
import { serveStatic } from "./check-serve.mjs";

export function compareLighthouse(
  results,
  baseline,
  { checkPerformance = false } = {},
) {
  const failures = [];
  for (const expected of baseline.pages) {
    const result = results.find((entry) => entry.path === expected.new);
    if (!result) {
      failures.push(`missing Lighthouse result: ${expected.new}`);
      continue;
    }
    if (result.accessibility < expected.accessibility) {
      failures.push(
        `${expected.new} accessibility ${result.accessibility} is below baseline ${expected.accessibility}`,
      );
    }
    if (checkPerformance && result.performance < expected.performance) {
      failures.push(
        `${expected.new} performance ${result.performance} is below baseline ${expected.performance}`,
      );
    }
  }
  return failures;
}

export async function auditPages({ baseUrl, baseline, chromePath }) {
  const chrome = await launch({
    chromePath,
    chromeFlags: ["--headless", "--no-sandbox"],
  });
  try {
    const results = [];
    for (const page of baseline.pages) {
      const run = await lighthouse(new URL(page.new, baseUrl).href, {
        port: chrome.port,
        output: "json",
        onlyCategories: ["accessibility", "performance"],
        logLevel: "error",
      });
      results.push({
        path: page.new,
        accessibility: run.lhr.categories.accessibility.score,
        performance: run.lhr.categories.performance.score,
      });
    }
    return results;
  } finally {
    await chrome.kill();
  }
}

if (import.meta.url === new URL(`file://${process.argv[1]}`).href) {
  const baseline = JSON.parse(
    await readFile(
      resolve(import.meta.dirname, "../parity/lighthouse-baseline.json"),
      "utf8",
    ),
  );
  const baseUrl = process.env.DOCS_BASE_URL ?? "http://127.0.0.1:4322";
  const server = process.env.DOCS_BASE_URL
    ? undefined
    : await serveStatic({ root: resolve(import.meta.dirname, "../_site") });
  const results = await auditPages({
    baseUrl,
    baseline,
    chromePath: process.env.CHROME_PATH ?? chromium.executablePath(),
  }).finally(() => server?.close());
  console.table(results);
  const failures = compareLighthouse(results, baseline, {
    checkPerformance: process.env.LIGHTHOUSE_CUTOVER === "1",
  });
  if (failures.length) {
    console.error(failures.join("\n"));
    process.exitCode = 1;
  }
}
