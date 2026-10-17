import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  chmodSync,
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import test from "node:test";

const repository = resolve(import.meta.dirname, "../..");
const helper =
  process.env.TEST_SOURCE_LINT_HELPER ??
  join(repository, "dev/js/lint-source.mjs");
const oxlint = join(repository, "node_modules/.bin/oxlint");

function runFixture(files, localConfig) {
  const root = realpathSync(
    mkdtempSync(join(tmpdir(), "xmtp-source-lint-test-")),
  );
  const browser = join(root, "sdks/browser");
  function write(path, content) {
    const destination = join(root, path);
    mkdirSync(dirname(destination), { recursive: true });
    writeFileSync(destination, content);
  }
  try {
    mkdirSync(join(root, "dev/js"), { recursive: true });
    mkdirSync(browser, { recursive: true });
    copyFileSync(helper, join(root, "dev/js/lint-source.mjs"));
    copyFileSync(
      join(repository, ".oxlintrc.json"),
      join(root, ".oxlintrc.json"),
    );
    if (localConfig)
      write("sdks/browser/.oxlintrc.json", JSON.stringify(localConfig));
    for (const [path, content] of Object.entries(files)) write(path, content);
    // Only workspace discovery is replaced. Each lint call uses the pinned Oxlint.
    write(
      "bin/pnpm",
      `#!${process.execPath}
const { spawnSync } = require("node:child_process");
const args = process.argv.slice(2);
if (args[0] === "-r") {
  console.log(JSON.stringify(${JSON.stringify([
    { name: "@xmtp/workspace", path: root },
    { name: "@xmtp/browser-sdk", path: browser },
  ])}));
} else if (args[0] === "exec" && args[1] === "oxlint") {
  const result = spawnSync(${JSON.stringify(oxlint)}, args.slice(2), { stdio: "inherit" });
  process.exit(result.status ?? 1);
} else {
  throw new Error("Unexpected pnpm command");
}
`,
    );
    chmodSync(join(root, "bin/pnpm"), 0o755);
    const result = spawnSync(
      process.execPath,
      [join(root, "dev/js/lint-source.mjs")],
      {
        cwd: root,
        env: {
          ...process.env,
          PATH: `${join(root, "bin")}:${process.env.PATH}`,
        },
        encoding: "utf8",
      },
    );
    for (const directory of [root, browser]) {
      assert.deepEqual(
        readdirSync(directory).filter((name) =>
          name.startsWith(".oxlint-source-"),
        ),
        [],
      );
    }
    return { status: result.status, output: result.stdout + result.stderr };
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

const ignoredDirective =
  "// @ts-ignore\nexport const value: number = 'fixture';\n";

test("shared platform and generated exclusions keep their original scope", () => {
  const result = runFixture({
    "sdks/browser/test/platform/negative.ts": ignoredDirective,
    "sdks/browser/dist/generated.ts": ignoredDirective,
    "sdks/browser/src/valid.ts": "export const value = 1;\n",
  });
  assert.equal(result.status, 0, result.output);
});

test("shared rules still reject a directive in normal source", () => {
  const result = runFixture({
    "sdks/browser/src/negative.ts": ignoredDirective,
  });
  assert.equal(result.status, 1, result.output);
  assert.match(result.output, /ban-ts-comment/);
});

test("package ignore patterns and rule overrides keep their original scope", () => {
  const result = runFixture(
    {
      "sdks/browser/excluded/negative.ts": ignoredDirective,
      "sdks/browser/special/negative.ts": ignoredDirective,
    },
    {
      extends: ["../../.oxlintrc.json"],
      ignorePatterns: ["excluded/**"],
      overrides: [
        {
          files: ["special/**/*.ts"],
          rules: { "typescript/ban-ts-comment": "off" },
        },
      ],
    },
  );
  assert.equal(result.status, 0, result.output);
});
