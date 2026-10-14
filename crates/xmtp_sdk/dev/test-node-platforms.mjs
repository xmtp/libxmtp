import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  NODE_PLATFORMS,
  stageNodePlatforms,
  usePlatformPackages,
} from "./node-platforms.mjs";

const digest = (value) => createHash("sha256").update(value).digest("hex");
test("the public product contains all six verified library and runtime pairs", () => {
  const temporary = mkdtempSync(join(tmpdir(), "sdk-node-platforms-"));
  try {
    const input = join(temporary, "input");
    const output = join(temporary, "output");
    const runtime = join(output, "node_modules/@ubjs/node");
    mkdirSync(runtime, { recursive: true });
    writeFileSync(join(runtime, "package.json"), '{"files":["lib.js"]}');
    const binding = {
      generator: "generator",
      artifact: { source: "rust-source", features: "", profile: "release" },
    };
    const stage = (binding, revision = "pin", commit = "release") =>
      stageNodePlatforms(
        input,
        output,
        binding,
        revision,
        "@xmtp/node-sdk",
        commit,
      );
    const receiptFile = (target) =>
      join(input, target, "sdk-node-platform.json");
    const editReceipt = (target, edit) => {
      const receipt = JSON.parse(readFileSync(receiptFile(target)));
      writeFileSync(receiptFile(target), JSON.stringify(edit(receipt)));
    };
    for (const [target, [rustTarget, library, cargoBuilt]] of Object.entries(
      NODE_PLATFORMS,
    )) {
      const root = join(input, target);
      const addon = `uniffi-runtime-napi.${target}.node`;
      mkdirSync(root, { recursive: true });
      writeFileSync(join(root, library), `library-${target}`);
      writeFileSync(join(root, addon), `runtime-${target}`);
      writeFileSync(
        join(root, "sdk-node-platform.json"),
        JSON.stringify({
          schema: 1,
          target,
          rustTarget,
          // Nix pairs name the closure identity; the cargo pair, its commit.
          ...(cargoBuilt ? { commit: "release" } : { source: "rust-source" }),
          features: "",
          profile: "release",
          runtimeRevision: "pin",
          files: {
            [library]: digest(`library-${target}`),
            [addon]: digest(`runtime-${target}`),
          },
        }),
      );
    }
    const staged = stage(binding);
    assert.equal(Object.keys(staged.platforms).length, 6);
    for (const [target, [, library]] of Object.entries(NODE_PLATFORMS)) {
      assert.equal(
        readFileSync(join(output, "native", target, library), "utf8"),
        `library-${target}`,
      );
      assert.equal(
        readFileSync(
          join(runtime, `uniffi-runtime-napi.${target}.node`),
          "utf8",
        ),
        `runtime-${target}`,
      );
      assert.equal(
        staged.exports[`./native/${target}/package.json`],
        `./native/${target}/package.json`,
      );
    }
    const manifest = JSON.parse(readFileSync(join(runtime, "package.json")));
    assert.ok(
      manifest.files.includes("uniffi-runtime-napi.win32-x64-msvc.node"),
    );
    assert.throws(
      () =>
        stage({
          ...binding,
          artifact: { ...binding.artifact, source: "other" },
        }),
      /provenance mismatch: darwin-arm64/,
    );
    assert.throws(() => stage(binding, "other-pin"), /provenance mismatch/);
    assert.throws(
      () => stage(binding, "pin", "other-commit"),
      /provenance mismatch: win32-x64-msvc/,
    );
    editReceipt("win32-x64-msvc", (receipt) => ({
      ...receipt,
      source: "rust-source",
    }));
    assert.throws(() => stage(binding), /provenance mismatch: win32-x64-msvc/);
    editReceipt("win32-x64-msvc", ({ source: _, ...receipt }) => receipt);
    editReceipt("linux-x64-musl", ({ source: _, ...receipt }) => ({
      ...receipt,
      commit: "release",
    }));
    assert.throws(() => stage(binding), /provenance mismatch: linux-x64-musl/);
    editReceipt("linux-x64-musl", ({ commit: _, ...receipt }) => ({
      ...receipt,
      source: "rust-source",
    }));
    stage(binding);
    writeFileSync(join(input, "win32-x64-msvc/xmtp_sdk.dll"), "changed bytes");
    assert.throws(
      () => stage(binding),
      /asset mismatch: win32-x64-msvc\/xmtp_sdk.dll/,
    );
    rmSync(join(input, "darwin-arm64"), { recursive: true });
    assert.throws(() => stage(binding), /ENOENT/);
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
});

test("the generated library call uses supported public package subpaths", () => {
  const source =
    'const libPath = resolveLibPath({crateName: "xmtp_sdk", callerUrl: import.meta.url});';
  const product = usePlatformPackages(source, "@xmtp/node-sdk");
  const resolveLibPath = (options) => options;
  const options = Function(
    "resolveLibPath",
    product.replace("import.meta.url", '"caller"') + "return libPath;",
  )(resolveLibPath);
  assert.deepEqual(options, {
    crateName: "xmtp_sdk",
    callerUrl: "caller",
    npmPackageBase: "@xmtp/node-sdk/native/",
    tripleStyle: "node",
  });
  assert.throws(
    () => usePlatformPackages("unknown loader", "@xmtp/node-sdk"),
    /resolver changed/,
  );
  assert.throws(
    () => usePlatformPackages(source + source, "@xmtp/node-sdk"),
    /resolver changed/,
  );
});
