// Runs in the installed consumer against the package's private public entry.
//
// 1. The entry exports exactly the public names: the object and error classes
//    and public functions of the generated values file, and the classes and
//    functions of the public runtime. The expected list comes from those
//    files, not from the entry, so a dropped name or a star re-export of the
//    internal conversions fails here.
// 2. A second copy of the package in the process fails at load with a public
//    error (Decision 20), before it replaces the first copy's native callback
//    tables. Both second copies are checked: CommonJS require under a
//    TypeScript loader, and a worker thread, which has its own globalThis.
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { Worker } from "node:worker_threads";

import * as imported from "xmtp-sdk/public";

const require = createRequire(import.meta.url);
const packageRoot = dirname(
  fileURLToPath(import.meta.resolve("xmtp-sdk/public")),
);

// Conversion and wiring helpers that the entry must not export.
const INTERNAL = new Set([
  "currentProjection",
  "installProjection",
  "publicError",
  "attachClientBinding",
  "clientBinding",
  "publicClient",
  "liftBoundMessage",
  "boundMessage",
  "publicEventStream",
  "hostOptions",
  "ClientMembers",
  "ObjectProjection",
  "StandardCodec",
]);
function exportedValues(source: string): string[] {
  return [
    ...source.matchAll(
      /^export (?:abstract )?(?:async )?(?:class|function) (\w+)/gm,
    ),
  ]
    .map((match) => match[1]!)
    .filter(
      (name) =>
        !INTERNAL.has(name) && !/^(?:lift|lower|wrap|unwrap)[A-Z]/.test(name),
    );
}
const expected = new Set([
  ...exportedValues(
    readFileSync(join(packageRoot, "public-values.gen.ts"), "utf8"),
  ),
  ...readdirSync(join(packageRoot, "runtime/public"))
    .filter((file) => file.endsWith(".ts"))
    .flatMap((file) =>
      exportedValues(
        readFileSync(join(packageRoot, "runtime/public", file), "utf8"),
      ),
    ),
  "Timestamp",
]);
assert.ok(expected.has("Preferences") && expected.has("Client"));
assert.deepEqual(
  Object.keys(imported).sort(),
  [...expected].sort(),
  "the public entry does not export exactly the public names",
);

// A public error narrows with the package's classes.
try {
  new imported.MarkdownCodec().decode(new imported.TextCodec().encode("text"));
  assert.fail("a wrong codec decoded");
} catch (error) {
  assert.ok(error instanceof imported.XmtpError.InvalidArgument);
  assert.equal(error.details.category, "input");
}

// A second copy fails at load, before it registers anything.
const twice =
  "the XMTP SDK was loaded twice in one process; load it once, from one thread";
assert.throws(
  () => require("xmtp-sdk/public"),
  (error: unknown) => {
    assert.ok(error instanceof Error);
    assert.equal(error.name, "XmtpError.Unknown");
    assert.equal(error.message, twice);
    return true;
  },
);
const workerError = await new Promise<unknown>((resolve, reject) => {
  const worker = new Worker(new URL("./worker-load.mts", import.meta.url), {
    execArgv: process.execArgv,
  });
  const deadline = setTimeout(() => {
    void worker.terminate();
    reject(new Error("the worker load did not finish"));
  }, 30_000);
  worker.once("error", (error) => {
    clearTimeout(deadline);
    resolve(error);
  });
  worker.once("exit", () => {
    clearTimeout(deadline);
    resolve(undefined);
  });
});
assert.ok(workerError instanceof Error, "a worker thread loaded a second copy");
assert.equal(workerError.message, twice);
// The loaded copy keeps working.
assert.equal(imported.encodeText("still loaded").type.typeId, "text");
console.log(
  `Node private entry: ${expected.size} public names; a second copy fails at load`,
);
