import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import {
  approvedPureFunctions,
  currentPureFunctions,
} from "./check-pure-wasm.mjs";

const expected = currentPureFunctions();
assert.ok(expected.includes("generateInboxId"));
assert.ok(expected.includes("remoteAttachmentFromEncrypted"));
assert.ok(expected.includes("catalogueContentTypeShouldPush"));
const root = mkdtempSync(join(tmpdir(), "sdk-pure-export-gate-"));
const validator = fileURLToPath(
  new URL("./check-pure-wasm.mjs", import.meta.url),
);
const pure = join(root, "typescript-pure/xmtp_sdk.ts");
const worker = join(root, "typescript-wasm/xmtp_sdk.ts");
const dispatch = join(root, "typescript-wasm/dispatch.gen.ts");
const functions = (names) =>
  names.map((name) => `export function ${name}() {}\n`).join("");
const run = () =>
  spawnSync(process.execPath, [validator, root], { encoding: "utf8" });
const reset = () => {
  writeFileSync(pure, functions(expected));
  writeFileSync(worker, "export async function createClient() {}\n");
  writeFileSync(dispatch, "{}\n");
};
const rejects = (reason) => {
  const result = run();
  assert.notEqual(
    result.status,
    0,
    `${reason}: validator accepted invalid output`,
  );
  assert.ok(result.stderr.includes(reason), result.stderr);
};
try {
  const runtime = join(root, "typescript-pure/runtime");
  mkdirSync(join(runtime, "public"), { recursive: true });
  mkdirSync(join(root, "typescript-wasm"));
  for (const name of ["codec-type.ts", "codecs.ts", "ids.ts", "index.ts"])
    writeFileSync(join(runtime, name), "");
  for (const name of ["codec.ts", "codecs.ts"])
    writeFileSync(join(runtime, "public", name), "");
  reset();
  assert.equal(run().status, 0, "approved current source exports rejected");
  for (const name of expected) {
    writeFileSync(pure, functions(expected.filter((value) => value !== name)));
    rejects("pure WASM export set changed");
    reset();
    writeFileSync(worker, `export async function ${name}() {}\n`);
    rejects(`${name} reached worker WASM`);
    reset();
    writeFileSync(dispatch, JSON.stringify({ [name]: {} }));
    rejects(`${name} reached worker dispatch`);
    reset();
  }
  writeFileSync(pure, functions([...expected, "createClient"]));
  rejects("pure WASM export set changed");
  reset();
  writeFileSync(
    pure,
    functions(expected).replace("export function", "export async function"),
  );
  rejects("pure WASM function is async");
  reset();
  const source = expected
    .map(
      (name) =>
        `#[xmtp_macro::sdk_export(pure)]\npub fn ${name.replace(/[A-Z]/g, (letter) => `_${letter.toLowerCase()}`)}() {}\n`,
    )
    .join("");
  assert.deepEqual(approvedPureFunctions([source]), expected);
  assert.throws(
    () =>
      approvedPureFunctions([
        source.replace("pub fn generate_inbox_id", "pub fn missing_inbox_id"),
      ]),
    /unapproved pure function/,
  );
  assert.throws(
    () =>
      approvedPureFunctions([
        source.replace(
          "#[xmtp_macro::sdk_export(pure)]\npub fn generate_inbox_id() {}\n",
          "",
        ),
      ]),
    /missing approved pure Rust function/,
  );
  assert.throws(
    () =>
      approvedPureFunctions([
        source,
        "#[xmtp_macro::sdk_export(pure)]\npub fn create_client() {}\n",
      ]),
    /unapproved pure function/,
  );
  const envelope =
    "#[xmtp_macro::sdk_export(pure)]\npub fn decode_encoded_content() {}\n#[xmtp_macro::sdk_export(pure)]\npub fn encode_encoded_content() {}\n";
  const withoutEnvelope = source.replace(
    /#\[xmtp_macro::sdk_export\(pure\)\]\npub fn (?:decode|encode)_encoded_content\(\) \{\}\n/g,
    "",
  );
  assert.deepEqual(
    approvedPureFunctions([withoutEnvelope, envelope]),
    [
      ...new Set([...expected, "decodeEncodedContent", "encodeEncodedContent"]),
    ].sort(),
  );
  assert.throws(
    () =>
      approvedPureFunctions([
        withoutEnvelope,
        envelope
          .split("#[xmtp_macro::sdk_export(pure)]")
          .slice(0, 2)
          .join("#[xmtp_macro::sdk_export(pure)]"),
      ]),
    /incomplete envelope conversion pair/,
  );
  const privateSample =
    '#[cfg(feature = "conformance")]\n#[xmtp_macro::sdk_export(pure)]\npub fn sdk_conformance_standard_samples() {}\n';
  assert.deepEqual(approvedPureFunctions([source, privateSample]), expected);
  assert.throws(
    () =>
      approvedPureFunctions([
        source,
        privateSample.replace('#[cfg(feature = "conformance")]\n', ""),
      ]),
    /unapproved pure function/,
  );
  assert.equal(run().status, 0, "exact restored fixture rejected");
  console.log(
    `Pure export gate PASS: ${expected.length} source functions; missing/extra, worker/dispatch leaks, async and source contract controls`,
  );
} finally {
  rmSync(root, { recursive: true, force: true });
}
