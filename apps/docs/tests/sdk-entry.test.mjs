import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { sdkEntry } from "../scripts/sdk-entry.mjs";

test("SDK entries support each isolated cutover graph", () => {
  const root = mkdtempSync(join(tmpdir(), "xmtp-docs-entries-"));
  try {
    for (const generated of ["node", "browser", "react-native"]) {
      for (const sdk of ["node", "browser", "react-native"]) {
        const packageRoot = join(root, sdk);
        mkdirSync(packageRoot, { recursive: true });
        writeFileSync(
          join(packageRoot, "package.json"),
          JSON.stringify({
            exports: { "./react": { types: "./dist/react/index.d.ts" } },
            scripts: {
              build:
                sdk === generated
                  ? `../../dev/js/sdk-package ${sdk}`
                  : "tsdown",
            },
          }),
        );
        assert.equal(
          sdkEntry(packageRoot, false, "./react"),
          join(packageRoot, "dist/react/index.d.ts"),
        );
        assert.throws(
          () => sdkEntry(packageRoot, false, "./private"),
          /SDK declaration export is missing/,
        );
        assert.equal(
          sdkEntry(packageRoot),
          join(
            packageRoot,
            sdk === generated ? "dist/entry.d.ts" : "dist/index.d.ts",
          ),
        );
        assert.equal(
          sdkEntry(packageRoot, true),
          join(
            packageRoot,
            sdk === generated ? "dist/entry.d.ts" : "src/index.ts",
          ),
        );
      }
    }
  } finally {
    rmSync(root, { recursive: true });
  }
});
