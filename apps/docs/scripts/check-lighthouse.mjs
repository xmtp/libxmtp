import { accessSync, constants, readFileSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { dirname, join, resolve, sep } from "node:path";

import { launch } from "chrome-launcher";
import lighthouse from "lighthouse";
import { chromium } from "playwright";

import { serveStatic } from "./check-serve.mjs";

function playwrightBrowsers() {
  const require = createRequire(import.meta.url);
  const core = createRequire(require.resolve("playwright")).resolve(
    "playwright-core",
  );
  return JSON.parse(readFileSync(join(dirname(core), "browsers.json"), "utf8"))
    .browsers;
}

export function selectChromePath({
  chromePath,
  platform = process.platform,
  arch = process.arch,
  fullPath = chromium.executablePath(),
  browsers,
} = {}) {
  if (chromePath !== undefined) return chromePath;
  if (platform !== "darwin") return fullPath;

  const manifest = browsers ?? playwrightBrowsers();
  const full = manifest.find((browser) => browser.name === "chromium");
  const shell = manifest.find(
    (browser) => browser.name === "chromium-headless-shell",
  );
  const installHelp = "Run dev/nix-shell 'just docs browsers'.";
  if (
    !["arm64", "x64"].includes(arch) ||
    !full?.revision ||
    !full.browserVersion ||
    full.revision !== shell?.revision ||
    full.browserVersion !== shell.browserVersion
  ) {
    throw new Error(
      `Playwright Chromium versions do not match. ${installHelp}`,
    );
  }

  // Use the layout from this Playwright package and its installed revision.
  const suffix = join(
    `chromium-${full.revision}`,
    `chrome-mac-${arch}`,
    "Google Chrome for Testing.app",
    "Contents",
    "MacOS",
    "Google Chrome for Testing",
  );
  if (!fullPath.endsWith(`${sep}${suffix}`)) {
    throw new Error(`Unknown Playwright Chromium installation. ${installHelp}`);
  }
  const shellPath = join(
    fullPath.slice(0, -suffix.length),
    `chromium_headless_shell-${shell.revision}`,
    `chrome-headless-shell-mac-${arch}`,
    "chrome-headless-shell",
  );
  try {
    accessSync(shellPath, constants.X_OK);
  } catch {
    throw new Error(`Playwright headless shell is missing. ${installHelp}`);
  }
  return shellPath;
}

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
  const chromePath = selectChromePath({ chromePath: process.env.CHROME_PATH });
  const baseUrl = process.env.DOCS_BASE_URL ?? "http://127.0.0.1:4322";
  const server = process.env.DOCS_BASE_URL
    ? undefined
    : await serveStatic({ root: resolve(import.meta.dirname, "../_site") });
  const results = await auditPages({
    baseUrl,
    baseline,
    chromePath,
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
