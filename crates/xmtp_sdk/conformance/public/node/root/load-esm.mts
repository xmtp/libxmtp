// Check the installed ESM root and its public error types.
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import * as imported from "xmtp-sdk";

const packageRoot =
  process.env.SDK_EXPECTED_NODE_PATH ??
  dirname(fileURLToPath(import.meta.resolve("xmtp-sdk")));

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
  "encodeForSend",
  "optionsForSend",
  "contentForSend",
  "codecType",
  "isCodec",
]);
function exportedValues(source: string): string[] {
  return [
    ...source.matchAll(
      /^export (?:abstract )?(?:async )?(?:class|function) (\w+)/gm,
    ),
  ]
    .map((match) => match[1])
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
  Object.keys(imported).sort((left, right) => left.localeCompare(right)),
  [...expected].sort((left, right) => left.localeCompare(right)),
  "the ESM package root does not export exactly the public names",
);
try {
  new imported.MarkdownCodec().decode(new imported.TextCodec().encode("text"));
  assert.fail("a wrong codec decoded");
} catch (error) {
  assert.ok(error instanceof imported.XmtpError.InvalidArgument);
  assert.equal(error.details.category, "input");
}
console.log(`Node ESM package root: ${expected.size} public names`);
