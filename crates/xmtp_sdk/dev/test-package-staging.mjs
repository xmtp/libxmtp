import assert from "node:assert/strict";
import fs from "node:fs";
import { syncBuiltinESMExports } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { removeTree, stagePackage } from "./package-staging.mjs";

test("failed replacement and rollback leave the prior product recoverable", () => {
  const temporary = fs.mkdtempSync(join(tmpdir(), "sdk-stage-recovery-"));
  const product = join(temporary, "react-native");
  fs.mkdirSync(product);
  const previousBytes = Buffer.from("export const version = 'previous';\n");
  fs.writeFileSync(join(product, "entry.js"), previousBytes);
  const originalRename = fs.renameSync;
  const replacementError = new Error("replacement blocked");
  const rollbackError = new Error("rollback blocked");
  let previous;
  try {
    fs.renameSync = (source, destination) => {
      if (destination === product)
        throw source === previous ? rollbackError : replacementError;
      return originalRename(source, destination);
    };
    syncBuiltinESMExports();
    assert.throws(
      () =>
        stagePackage(product, (destination, staging) => {
          previous = join(staging, "previous");
          fs.mkdirSync(destination);
          fs.writeFileSync(join(destination, "entry.js"), "replacement");
        }),
      (error) => {
        assert.ok(error instanceof AggregateError);
        assert.deepEqual(error.errors, [replacementError, rollbackError]);
        assert.ok(error.message.includes(previous));
        return true;
      },
    );
    assert.equal(fs.existsSync(product), false);
    assert.deepEqual(
      fs.readFileSync(join(previous, "entry.js")),
      previousBytes,
    );
    // Recovery must work with the surviving bytes, without rebuilding anything.
    originalRename(previous, product);
    assert.deepEqual(fs.readFileSync(join(product, "entry.js")), previousBytes);
  } finally {
    fs.renameSync = originalRename;
    syncBuiltinESMExports();
    removeTree(temporary);
  }
});
