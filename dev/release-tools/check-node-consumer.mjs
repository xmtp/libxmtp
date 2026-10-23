import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import {
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  realpathSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const [consumerFile, nodeFile, sdk = "agent-sdk"] = process.argv.slice(2);
assert.ok(sdk === "agent-sdk" || sdk === "cli", "Expected Agent or CLI");
const consumer = resolve(consumerFile);
const node = resolve(nodeFile);
const root = fileURLToPath(new URL("../..", import.meta.url));
const directory = mkdtempSync(join(tmpdir(), "xmtp-installed-consumer-"));
try {
  const workspace = join(directory, "workspace");
  mkdirSync(workspace);
  for (const file of ["package.json", "pnpm-workspace.yaml", "pnpm-lock.yaml"])
    copyFileSync(join(root, file), join(workspace, file));
  const packages = [
    [sdk === "cli" ? "apps/cli" : "sdks/agent", consumer],
    ["sdks/node", node],
  ];
  for (const [path] of packages) {
    const source = join(workspace, path);
    mkdirSync(source, { recursive: true });
    copyFileSync(
      join(root, path, "package.json"),
      join(source, "package.json"),
    );
    if (path !== "sdks/node") {
      const manifest = JSON.parse(
        readFileSync(join(source, "package.json"), "utf8"),
      );
      if (
        typeof manifest.version === "string" &&
        manifest.dependencies?.["@xmtp/node-sdk"] === manifest.version
      ) {
        manifest.dependencies["@xmtp/node-sdk"] = "workspace:*";
        writeFileSync(join(source, "package.json"), JSON.stringify(manifest));
      }
    }
  }
  execFileSync(
    "pnpm",
    [
      "--dir",
      workspace,
      "install",
      "--prod",
      "--frozen-lockfile",
      "--ignore-scripts",
    ],
    { stdio: "inherit" },
  );
  for (const [path, archive] of packages) {
    const source = join(workspace, path);
    const expected = join(directory, path);
    mkdirSync(expected, { recursive: true });
    execFileSync(
      "pnpm",
      [
        "--dir",
        source,
        "pack",
        "--config.ignore-scripts=true",
        "--pack-destination",
        expected,
      ],
      { stdio: "inherit" },
    );
    const [packed] = readdirSync(expected);
    const original = JSON.parse(
      execFileSync(
        "tar",
        ["-xOf", join(expected, packed), "package/package.json"],
        {
          encoding: "utf8",
        },
      ),
    );
    const retained = JSON.parse(
      execFileSync("tar", ["-xOf", archive, "package/package.json"], {
        encoding: "utf8",
      }),
    );
    assert.equal(retained.name, original.name, "Unexpected retained package");
    assert.equal(retained.version, original.version, "Expected frozen version");
    for (const field of [
      "dependencies",
      "optionalDependencies",
      "peerDependencies",
    ]) {
      const dependencies = { ...retained[field] };
      if (path === "sdks/node")
        for (const name of retained.bundledDependencies ?? [])
          delete dependencies[name];
      assert.deepEqual(
        dependencies,
        original[field] ?? {},
        "Retained dependencies must match the reviewed workspace",
      );
    }
  }
  const installed = join(directory, "installed");
  execFileSync(
    "pnpm",
    [
      "--dir",
      workspace,
      "--filter",
      `@xmtp/${sdk}`,
      "deploy",
      "--prod",
      "--frozen-lockfile",
      "--ignore-scripts",
      installed,
    ],
    { stdio: "inherit" },
  );
  const installedNode = realpathSync(
    join(installed, "node_modules/@xmtp/node-sdk"),
  );
  rmSync(installedNode, { recursive: true, force: true });
  mkdirSync(installedNode);
  for (const file of readdirSync(installed))
    if (file !== "node_modules")
      rmSync(join(installed, file), { recursive: true, force: true });
  for (const [archive, destination] of [
    [consumer, installed],
    [node, installedNode],
  ])
    execFileSync("tar", [
      "-xzf",
      archive,
      "--strip-components=1",
      "-C",
      destination,
    ]);
  mkdirSync(join(directory, "node_modules/@xmtp"), { recursive: true });
  symlinkSync(installed, join(directory, `node_modules/@xmtp/${sdk}`), "dir");
  symlinkSync(
    installedNode,
    join(directory, "node_modules/@xmtp/node-sdk"),
    "dir",
  );
  writeFileSync(
    join(directory, "package.json"),
    JSON.stringify({
      private: true,
      type: "module",
    }),
  );
  /** @type {unknown} */
  const manifest = JSON.parse(
    readFileSync(join(installed, "package.json"), "utf8"),
  );
  if (
    typeof manifest !== "object" ||
    manifest === null ||
    !("version" in manifest) ||
    typeof manifest.version !== "string" ||
    !("dependencies" in manifest) ||
    typeof manifest.dependencies !== "object" ||
    manifest.dependencies === null ||
    !("@xmtp/node-sdk" in manifest.dependencies)
  )
    throw new Error(
      "Installed consumer package has invalid version/dependency metadata",
    );
  const expected = manifest.version;
  assert.equal(
    manifest.dependencies["@xmtp/node-sdk"],
    expected,
    "Consumer must pin the checked Node version",
  );
  const consumerCheck =
    sdk === "agent-sdk"
      ? "import * as agent from '@xmtp/agent-sdk';\nassert.equal(typeof agent.Agent, 'function');\n"
      : "";
  writeFileSync(
    join(directory, "smoke.mjs"),
    `import assert from 'node:assert/strict';\nimport {sdkVersion} from '@xmtp/node-sdk';\nassert.equal(sdkVersion(), ${JSON.stringify(expected)});\n${consumerCheck}`,
  );
  if (sdk === "cli") {
    const bin = join(directory, "node_modules/@xmtp/cli/bin/run.js");
    const output = execFileSync(process.execPath, [bin, "--version"], {
      cwd: directory,
      encoding: "utf8",
    });
    assert.ok(
      output.includes(`@xmtp/cli/${expected} `),
      "CLI must report the shared version",
    );
    execFileSync(process.execPath, [bin, "--help"], {
      cwd: directory,
      stdio: "inherit",
    });
  }
  execFileSync(process.execPath, ["smoke.mjs"], {
    cwd: directory,
    stdio: "inherit",
  });
} finally {
  rmSync(directory, { recursive: true, force: true });
}
