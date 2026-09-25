import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";

const root = "target/sdk-generated";
const pure = readFileSync(`${root}/typescript-pure/xmtp_sdk.ts`, "utf8");
const worker = readFileSync(`${root}/typescript-wasm/xmtp_sdk.ts`, "utf8");
const dispatch = readFileSync(`${root}/typescript-wasm/dispatch.gen.ts`, "utf8");
const publicFunctions = (source) =>
  [...source.matchAll(/^export function (\w+)\(/gm)].map((match) => match[1]).sort();
// verifies: P70
const expected = [
  "decodeStandard",
  "encodeStandard",
  "encodeText",
  "sdkVersion",
  "standardContentType",
];
assert.deepEqual(publicFunctions(pure), expected, "pure WASM export set changed");
for (const name of expected) {
  assert.ok(!publicFunctions(worker).includes(name), `${name} reached worker WASM`);
  assert.ok(!dispatch.includes(`"${name}"`), `${name} reached worker dispatch`);
}
assert.deepEqual(
  readdirSync(`${root}/typescript-pure/runtime`).sort(),
  ["codec-type.ts", "codecs.ts", "ids.ts", "index.ts"],
);
console.log("pure WASM exports only the five approved functions");
