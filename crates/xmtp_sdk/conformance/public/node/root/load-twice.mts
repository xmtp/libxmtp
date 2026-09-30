// Runs in the installed consumer against the package root, through ESM import
// and through CommonJS require.
//
// Checked: each load exports exactly the public names, which are the object
// and error classes and public functions of the generated values file and the
// classes and functions of the public runtime. The expected list comes from
// those files, not from the entry, so a dropped name or a star re-export of the
// internal conversions fails here. A public error narrows with `instanceof`
// within one load.
//
// Reported, not asserted (known issue, see the Task 4 report): under a
// TypeScript loader, require() loads a second copy of the package. The copies
// are separate module instances, and a second copy re-registers the process-
// wide native callback tables.
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import * as imported from "xmtp-sdk";

const require = createRequire(import.meta.url);
const packageRoot = dirname(
  fileURLToPath(import.meta.resolve("xmtp-sdk")),
);

// Conversion and wiring helpers that the root must not export.
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
  "boundMessageOf",
  "checkStorage",
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
const required: typeof imported = require("xmtp-sdk");
for (const [load, sdk] of [
  ["import", imported],
  ["require", required],
] as const) {
  assert.deepEqual(
    Object.keys(sdk).sort(),
    [...expected].sort(),
    `the package root (${load}) does not export exactly the public names`,
  );
  // A public error narrows with the classes of the load that made it.
  try {
    new sdk.MarkdownCodec().decode(new sdk.TextCodec().encode("text"));
    assert.fail("a wrong codec decoded");
  } catch (error) {
    assert.ok(error instanceof sdk.XmtpError.InvalidArgument);
    assert.equal(error.details.category, "input");
  }
}
const shared = required.Client === imported.Client;
console.log(
  `Node package root: ${expected.size} public names through import and require; one module: ${shared}`,
);
