import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync, rmSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const [consumerFile, nodeFile, sdk = "agent-sdk"] = process.argv.slice(2);
assert.ok(sdk === "agent-sdk" || sdk === "cli", "Expected Agent or CLI");
const consumer = resolve(consumerFile);
const node = resolve(nodeFile);
const directory = mkdtempSync(join(tmpdir(), "xmtp-installed-consumer-"));
try {
  writeFileSync(
    join(directory, "package.json"),
    JSON.stringify({
      private: true,
      type: "module",
      dependencies: { "@xmtp/node-sdk": `file:${node}` },
      overrides: { "@xmtp/node-sdk": "$@xmtp/node-sdk" },
    }),
  );
  execFileSync(
    "npm",
    ["install", "--ignore-scripts", "--no-audit", "--no-fund", consumer],
    { cwd: directory, stdio: "inherit" },
  );
  /** @type {unknown} */
  const manifest = JSON.parse(
    readFileSync(
      join(directory, `node_modules/@xmtp/${sdk}/package.json`),
      "utf8",
    ),
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
