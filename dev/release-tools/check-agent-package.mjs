import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync, rmSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const [agent, node] = process.argv.slice(2).map((file) => resolve(file));
const directory = mkdtempSync(join(tmpdir(), "xmtp-installed-agent-"));
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
    ["install", "--ignore-scripts", "--no-audit", "--no-fund", agent],
    { cwd: directory, stdio: "inherit" },
  );
  /** @type {unknown} */
  const manifest = JSON.parse(
    readFileSync(
      join(directory, "node_modules/@xmtp/agent-sdk/package.json"),
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
      "Installed Agent package has invalid version/dependency metadata",
    );
  const expected = manifest.version;
  assert.equal(
    manifest.dependencies["@xmtp/node-sdk"],
    expected,
    "Agent must pin the checked Node version",
  );
  writeFileSync(
    join(directory, "smoke.mjs"),
    `import assert from 'node:assert/strict';\nimport * as agent from '@xmtp/agent-sdk';\nimport {sdkVersion} from '@xmtp/node-sdk';\nassert.equal(typeof agent.Agent, 'function');\nassert.equal(sdkVersion(), ${JSON.stringify(expected)});\n`,
  );
  execFileSync(process.execPath, ["smoke.mjs"], {
    cwd: directory,
    stdio: "inherit",
  });
} finally {
  rmSync(directory, { recursive: true, force: true });
}
