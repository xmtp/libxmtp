import assert from "node:assert/strict";
import {
  chmodSync,
  mkdirSync,
  mkdtempSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";

import {
  compareLighthouse,
  selectChromePath,
} from "../scripts/check-lighthouse.mjs";

const baseline = {
  pages: [{ new: "/", accessibility: 0.95, performance: 0.8 }],
};

test("Lighthouse comparison blocks accessibility regression", () => {
  assert.deepEqual(
    compareLighthouse(
      [{ path: "/", accessibility: 0.94, performance: 1 }],
      baseline,
    ),
    ["/ accessibility 0.94 is below baseline 0.95"],
  );
});

test("performance is blocking only for a cutover run", () => {
  const result = [{ path: "/", accessibility: 1, performance: 0.79 }];
  assert.deepEqual(compareLighthouse(result, baseline), []);
  assert.deepEqual(
    compareLighthouse(result, baseline, { checkPerformance: true }),
    ["/ performance 0.79 is below baseline 0.8"],
  );
});

function browserInstallation(t, arch = "arm64") {
  const root = mkdtempSync(join(tmpdir(), "docs-lighthouse-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const fullPath = join(
    root,
    "chromium-4567",
    `chrome-mac-${arch}`,
    "Google Chrome for Testing.app",
    "Contents",
    "MacOS",
    "Google Chrome for Testing",
  );
  const shellPath = join(
    root,
    "chromium_headless_shell-4567",
    `chrome-headless-shell-mac-${arch}`,
    "chrome-headless-shell",
  );
  for (const path of [fullPath, shellPath]) {
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, "fixture executable");
    chmodSync(path, 0o755);
  }
  const browsers = ["chromium", "chromium-headless-shell"].map((name) => ({
    name,
    revision: "4567",
    browserVersion: "150.0.1.2",
  }));
  return { fullPath, shellPath, browsers, platform: "darwin", arch };
}

test("Darwin Lighthouse uses the matching installed headless shell", (t) => {
  for (const arch of ["arm64", "x64"]) {
    const installation = browserInstallation(t, arch);
    assert.equal(selectChromePath(installation), installation.shellPath);
  }
});

test("explicit Chrome path takes priority on every platform", () => {
  for (const platform of ["darwin", "linux"]) {
    assert.equal(
      selectChromePath({
        platform,
        chromePath: "/chosen/browser",
        fullPath: "/other/browser",
        browsers: [],
      }),
      "/chosen/browser",
    );
  }
});

test("Linux retains the supplied Nix or Playwright Chromium path", () => {
  assert.equal(
    selectChromePath({
      platform: "linux",
      fullPath: "/nix/store/browser/bin/chromium",
      browsers: [],
    }),
    "/nix/store/browser/bin/chromium",
  );
});

test("Darwin rejects a missing shell and a stale installed revision", (t) => {
  const installation = browserInstallation(t);
  rmSync(installation.shellPath);
  assert.throws(
    () => selectChromePath(installation),
    /headless shell is missing/,
  );
  assert.throws(
    () =>
      selectChromePath({
        ...installation,
        fullPath: installation.fullPath.replace(
          "chromium-4567",
          "chromium-4566",
        ),
      }),
    /Unknown Playwright Chromium installation/,
  );
});

test("Darwin rejects mismatched package browser versions or revisions", (t) => {
  const installation = browserInstallation(t);
  for (const field of ["revision", "browserVersion"]) {
    const browsers = structuredClone(installation.browsers);
    browsers[1][field] = "different";
    assert.throws(
      () => selectChromePath({ ...installation, browsers }),
      /Chromium versions do not match/,
    );
  }
});
