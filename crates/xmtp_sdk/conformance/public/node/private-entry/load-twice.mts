// Runs in the installed consumer against the package's private public entry.
// It loads the entry through ESM import and through CommonJS require.
//
// Checked: CommonJS interop keeps every public name, and `instanceof` narrows
// a public error within each load. Not asserted (open question in the Task 4
// report): under a TypeScript loader, require() loads a second copy of the
// package, so a class from one load does not match a value from the other.
import assert from "node:assert/strict";
import { createRequire } from "node:module";

import * as imported from "xmtp-sdk/public";

const required: typeof imported = createRequire(import.meta.url)(
  "xmtp-sdk/public",
);

const names = [
  "Client",
  "Conversations",
  "Group",
  "Dm",
  "Message",
  "XmtpError",
  "MessageStream",
  "ConversationStream",
  "EventStream",
  "TextCodec",
  "Timestamp",
  "encodeText",
  "setLogSink",
] as const;
for (const name of names) {
  assert.equal(typeof imported[name], "function", `import lost ${name}`);
  assert.equal(typeof required[name], "function", `require lost ${name}`);
}
assert.deepEqual(
  Object.keys(required).sort(),
  Object.keys(imported).sort(),
  "CommonJS interop dropped public names",
);

// A public error narrows with the classes of the load that made it.
for (const sdk of [imported, required]) {
  let error: unknown;
  try {
    new sdk.MarkdownCodec().decode(new sdk.TextCodec().encode("text"));
  } catch (caught) {
    error = caught;
  }
  assert.ok(error instanceof sdk.XmtpError.InvalidArgument);
  assert.ok(error instanceof sdk.XmtpError);
  assert.equal(error.details.category, "input");
}
const shared = required.Client === imported.Client;
console.log(
  `Node private entry: all names through import and require; one module: ${shared}`,
);
