#!/usr/bin/env node
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
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
  const result = JSON.parse(
    execFileSync(
      "npm",
      [
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
  execFileSync(
    "npm",
    [
      "install",
      "--ignore-scripts",
      "--no-audit",
      "--no-fund",
      "--package-lock=false",
      join(consumer, result[0].filename),
    ],
    { cwd: consumer, stdio: "inherit" },
  );
  const packageRoot = join(consumer, "node_modules", manifest.name);
  const metadataFile = join(packageRoot, "sdk-contract.json");
  const original = readFileSync(metadataFile, "utf8");
  if (target === "node") {
    const proof = `import assert from 'node:assert/strict';\nimport { TextCodec } from '${manifest.name}';\nconst codec = new TextCodec(); assert.equal(codec.decode(codec.encode('installed')), 'installed');\nconsole.log('Installed SDK ESM codec round trip passed');\n`;
    writeFileSync(join(consumer, "smoke.mjs"), proof);
    execFileSync(process.execPath, ["smoke.mjs"], {
      cwd: consumer,
      stdio: "inherit",
    });
    const metadata = JSON.parse(original);
    metadata.contract = "deliberate-mismatch";
    writeFileSync(metadataFile, JSON.stringify(metadata));
    let failure;
    try {
      execFileSync(process.execPath, ["smoke.mjs"], {
        cwd: consumer,
        encoding: "utf8",
        stdio: "pipe",
      });
    } catch (error) {
      failure = error.stderr;
    }
    assert.match(failure ?? "", /SDK contract mismatch/);
    writeFileSync(metadataFile, original);
    const assetMetadata = JSON.parse(original);
    const firstAsset = Object.keys(assetMetadata.assets)[0];
    assetMetadata.assets[firstAsset] = "deliberate-asset-mismatch";
    writeFileSync(metadataFile, JSON.stringify(assetMetadata));
    let assetFailure;
    try {
      execFileSync(process.execPath, ["smoke.mjs"], {
        cwd: consumer,
        encoding: "utf8",
        stdio: "pipe",
      });
    } catch (error) {
      assetFailure = error.stderr;
    }
    assert.match(assetFailure ?? "", /SDK asset mismatch/);
    writeFileSync(metadataFile, original);
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
