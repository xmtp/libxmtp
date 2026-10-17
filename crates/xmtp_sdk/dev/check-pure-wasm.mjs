import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const sdkSource = resolve(dirname(fileURLToPath(import.meta.url)), "../src");
const required = [
  "catalogue_content_type_should_push",
  "decode_standard",
  "encode_standard",
  "encode_text",
  "generate_inbox_id",
  "is_catalogue_content_type",
  "metadata_field_ref",
  "remote_attachment_from_encrypted",
  "sdk_version",
  "standard_content_type",
];
// The Node/agent switch adds this pair. Other isolated SDK switches omit it.
const envelope = ["decode_encoded_content", "encode_encoded_content"];
const camel = (name) =>
  name.replace(/_([a-z])/g, (_, letter) => letter.toUpperCase());

function rustSources(root) {
  return readdirSync(root, { withFileTypes: true }).flatMap((entry) => {
    const path = join(root, entry.name);
    if (entry.isDirectory()) return rustSources(path);
    return entry.name.endsWith(".rs") ? [readFileSync(path, "utf8")] : [];
  });
}

export function approvedPureFunctions(sources) {
  const names = [];
  for (const source of sources) {
    for (const match of source.matchAll(
      /((?:^#\[[^\n]+\]\n)+)pub (async )?fn (\w+)\(/gm,
    )) {
      const [, attributes, asynchronous, name] = match;
      if (!attributes.includes("#[xmtp_macro::sdk_export(pure")) continue;
      if (
        name === "sdk_conformance_standard_samples" &&
        attributes.includes('#[cfg(feature = "conformance")]')
      )
        continue;
      assert.equal(asynchronous, undefined, `${name}: pure function is async`);
      assert.ok(
        [...required, ...envelope].includes(name),
        `${name}: unapproved pure function`,
      );
      names.push(name);
    }
  }
  assert.equal(
    new Set(names).size,
    names.length,
    "duplicate pure Rust function",
  );
  for (const name of required) {
    assert.ok(
      names.includes(name),
      `${name}: missing approved pure Rust function`,
    );
  }
  assert.equal(
    names.includes(envelope[0]),
    names.includes(envelope[1]),
    "incomplete envelope conversion pair",
  );
  return names.map(camel).sort();
}

const publicFunctions = (source) =>
  [...source.matchAll(/^export (?:async )?function (\w+)\(/gm)]
    .map((match) => match[1])
    .sort();

export function checkPureWasm(root, expected) {
  const pure = readFileSync(`${root}/typescript-pure/xmtp_sdk.ts`, "utf8");
  const worker = readFileSync(`${root}/typescript-wasm/xmtp_sdk.ts`, "utf8");
  const dispatch = readFileSync(
    `${root}/typescript-wasm/dispatch.gen.ts`,
    "utf8",
  );
  assert.ok(
    !/^export async function /m.test(pure),
    "pure WASM function is async",
  );
  assert.deepEqual(
    publicFunctions(pure),
    expected,
    "pure WASM export set changed",
  );
  for (const name of expected) {
    assert.ok(
      !publicFunctions(worker).includes(name),
      `${name} reached worker WASM`,
    );
    assert.ok(
      !dispatch.includes(`"${name}"`),
      `${name} reached worker dispatch`,
    );
  }
  assert.deepEqual(readdirSync(`${root}/typescript-pure/runtime`).sort(), [
    "codec-type.ts",
    "codecs.ts",
    "ids.ts",
    "index.ts",
    "public",
  ]);
  assert.deepEqual(
    readdirSync(`${root}/typescript-pure/runtime/public`).sort(),
    ["codec.ts", "codecs.ts"],
  );
}

export function currentPureFunctions() {
  return approvedPureFunctions(rustSources(sdkSource));
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  const expected = currentPureFunctions();
  checkPureWasm(process.argv[2] ?? "target/sdk-generated", expected);
  console.log(
    `Pure WASM exports exactly ${expected.length} approved source functions`,
  );
}
