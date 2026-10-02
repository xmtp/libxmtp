import assert from "node:assert/strict";
import { posix, win32 } from "node:path";
import { packageAsset } from "./package-smoke-path.mjs";

for (const [paths, root] of [[posix, "/pkg"], [win32, "C:\\pkg"]]) {
  for (const asset of ["/entry.js", "/typescript-wasm/xmtp_sdk.wasm", "/node_modules/@ubjs/core/dist/esm/index.js", "/snippets/space%20name.js"]) {
    assert.equal(packageAsset(root, asset, paths), paths.resolve(root, `.${decodeURIComponent(asset)}`));
  }
  for (const path of ["/", "/../outside.js", "/../pkg-other/entry.js", "/%2e%2e/outside.js"]) {
    assert.equal(packageAsset(root, path, paths), undefined);
  }
}
assert.equal(packageAsset("C:\\pkg", "/..%5Coutside.js", win32), undefined);
console.log("Package asset containment passed for POSIX and Windows paths");
