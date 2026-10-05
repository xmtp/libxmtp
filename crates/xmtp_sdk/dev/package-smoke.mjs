#!/usr/bin/env node
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import {
  mkdtempSync,
  readFileSync,
  writeFileSync,
  rmSync,
  existsSync,
  realpathSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve, dirname, delimiter } from "node:path";
// Windows npm.cmd is a shell entry. Run npm's JavaScript CLI through Node.
const npmCandidates = [
  process.env.XMTP_SDK_NPM_CLI
    ? resolve(process.env.XMTP_SDK_NPM_CLI)
    : undefined,
  join(dirname(process.execPath), "node_modules/npm/bin/npm-cli.js"),
  resolve(dirname(process.execPath), "../lib/node_modules/npm/bin/npm-cli.js"),
];
// Nix can install npm separately from the Node executable. Its npm link
// points to npm-cli.js. Read the link; never run it as a child process.
for (const directory of (process.env.PATH ?? "").split(delimiter)) {
  const link = join(directory, "npm");
  if (existsSync(link)) {
    const cli = realpathSync(link);
    if (cli.endsWith("npm-cli.js")) npmCandidates.push(cli);
  }
}
const npmCli = npmCandidates.find((path) => path && existsSync(path));
if (!npmCli) throw new Error("SDK package smoke cannot find npm-cli.js");
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
  execFileSync(
    process.execPath,
    [
      npmCli,
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
  // Build receipts must not be a runtime dependency.
  rmSync(metadataFile);
  assert.ok(!existsSync(join(packageRoot, "sdk-contract-check.js")));
  assert.ok(!existsSync(join(packageRoot, "sdk-pure-contract-check.js")));
  if (target === "node") {
    const proof = `import assert from 'node:assert/strict';\nimport { TextCodec } from '${manifest.name}';\nconst codec = new TextCodec(); assert.equal(codec.decode(codec.encode('installed')), 'installed');\nconsole.log('Installed SDK ESM codec round trip passed');\n`;
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
