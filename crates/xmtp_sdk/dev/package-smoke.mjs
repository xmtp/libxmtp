#!/usr/bin/env node
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import {
  mkdtempSync,
  readFileSync,
  writeFileSync,
  rmSync,
  existsSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import { npmCli as findNpmCli } from "./node-platforms.mjs";

const npmCli = findNpmCli();
const target = process.argv[2];
const staged = resolve(
  process.env.XMTP_SDK_PACKAGES_DIR ?? "target/sdk-packages",
  target,
);
const consumer = mkdtempSync(join(tmpdir(), "sdk-installed-"));
try {
  const manifest = JSON.parse(readFileSync(join(staged, "package.json")));
  assert.equal(manifest.type, "module");
  assert.equal(manifest.engines.node, ">=22.12.0");
  assert.ok(!JSON.stringify(manifest.exports).includes("require"));
  assert.ok(!JSON.stringify(manifest.exports).includes(".cjs"));
  writeFileSync(
    join(consumer, "package.json"),
    '{"private":true,"type":"module"}\n',
  );
  let archive = process.argv[3] ? resolve(process.argv[3]) : undefined;
  if (!archive) {
    const result = JSON.parse(
      execFileSync(
        process.execPath,
        [
          npmCli,
          "pack",
          staged,
          "--pack-destination",
          consumer,
          "--json",
          "--ignore-scripts",
        ],
        { encoding: "utf8" },
      ),
    );
    archive = join(consumer, result[0].filename);
  }
  execFileSync(
    process.execPath,
    [
      npmCli,
      "install",
      "--ignore-scripts",
      "--no-audit",
      "--no-fund",
      "--package-lock=false",
      archive,
    ],
    { cwd: consumer, stdio: "inherit" },
  );
  const packageRoot = join(consumer, "node_modules", manifest.name);
  const installedManifest = JSON.parse(
    readFileSync(join(packageRoot, "package.json")),
  );
  assert.equal(installedManifest.version, manifest.version);
  assert.equal(installedManifest.type, manifest.type);
  assert.equal(installedManifest.engines?.node, manifest.engines.node);
  assert.deepEqual(
    installedManifest.exports,
    manifest.exports,
    "Retained package public exports differ from the staged contract",
  );
  const metadataFile = join(packageRoot, "sdk-contract.json");
  // Build receipts must not be a runtime dependency.
  rmSync(metadataFile);
  assert.ok(!existsSync(join(packageRoot, "sdk-contract-check.js")));
  assert.ok(!existsSync(join(packageRoot, "sdk-pure-contract-check.js")));
  if (target === "node") {
    const proof = `import assert from 'node:assert/strict';\nimport { TextCodec, sdkVersion } from '${manifest.name}';\nassert.equal(sdkVersion(), ${JSON.stringify(manifest.version)});\nconst codec = new TextCodec(); assert.equal(codec.decode(codec.encode('installed')), 'installed');\nconsole.log('Installed SDK ESM codec round trip passed');\n`;
    writeFileSync(join(consumer, "smoke.mjs"), proof);
    execFileSync(process.execPath, ["smoke.mjs"], {
      cwd: consumer,
      stdio: "inherit",
    });
  } else {
    // Chromium runs the installed pure and worker roots with real assets.
    execFileSync(
      process.execPath,
      [resolve("crates/xmtp_sdk/dev/package-smoke-browser.mjs"), packageRoot],
      { stdio: "inherit" },
    );
  }
  console.log(`SDK installed ${target} package smoke passed`);
} finally {
  rmSync(consumer, { recursive: true, force: true });
}
