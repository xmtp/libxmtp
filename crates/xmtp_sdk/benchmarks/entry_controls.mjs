import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, mkdir, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const source = dirname(fileURLToPath(import.meta.url));
const directory = await mkdtemp(join(tmpdir(), "benchmark-entry-"));
const records = [];
try {
  for (const side of ["new", "old"]) {
    for (const target of ["node", "browser"]) {
      const root = join(directory, `${side}-${target}`);
      await mkdir(root);
      const manifest = {
        name:
          side === "old"
            ? `@xmtp/${target}-sdk`
            : target === "node"
              ? "xmtp-sdk"
              : "xmtp-sdk-browser",
        type: "module",
        exports: {
          ".": { types: "./types.d.ts", import: "./entry.mjs" },
          ...(target === "browser" && side === "new"
            ? { "./pure": { import: "./pure.mjs" } }
            : {}),
        },
      };
      for (const file of [
        "entry.mjs",
        "pure.mjs",
        "private.mjs",
        "accounts.mjs",
      ])
        await writeFile(join(root, file), "export const fixture = true;\n");
      await writeFile(
        join(root, "fixture.json"),
        JSON.stringify({ messages: [] }),
      );
      await writeFile(
        join(root, "vite.mjs"),
        "export async function createServer(){return {listen:async()=>{},close:async()=>{}}}",
      );
      await writeFile(
        join(root, "playwright.mjs"),
        "export const chromium={launchPersistentContext:async()=>({close:async()=>{},newPage:async()=>({on:()=>{},goto:async()=>{},waitForFunction:async()=>{},evaluate:async()=>({ready:true})})})};",
      );
      for (const fault of [
        "good",
        "private_pure",
        "private_root",
        "missing_pure",
        "null_pure",
        "root_codec",
        ...(side === "old" ? ["wrong_name", "wrong_type"] : []),
      ]) {
        await writeFile(
          join(root, "package.json"),
          JSON.stringify({
            ...manifest,
            ...(fault === "wrong_name" ? { name: "@xmtp/wrong-sdk" } : {}),
            ...(fault === "wrong_type" ? { type: "commonjs" } : {}),
          }),
        );
        const config = {
          sdk_entry: join(root, "entry.mjs"),
          accounts_entry: join(root, "accounts.mjs"),
          backend_url: "fixture",
          vite_entry: join(root, "vite.mjs"),
          playwright_entry: join(root, "playwright.mjs"),
          browser_port: 1,
          package_root: root,
          tools_root: root,
        };
        if (side === "old") config.pure_entry = config.sdk_entry;
        else if (target === "browser")
          config.pure_entry = join(root, "pure.mjs");
        if (fault === "private_pure")
          config.pure_entry = join(root, "private.mjs");
        if (fault === "private_root") {
          config.sdk_entry = join(root, "private.mjs");
          if (side === "old") config.pure_entry = config.sdk_entry;
        }
        if (fault === "missing_pure") delete config.pure_entry;
        if (fault === "null_pure") config.pure_entry = null;
        if (fault === "root_codec") config.pure_entry = config.sdk_entry;
        const configFile = join(root, "config.json");
        await writeFile(configFile, JSON.stringify(config));
        const request = {
          side,
          phase: "reset",
          workload: "cold_start",
          state_directory: root,
          package_root: root,
        };
        const child = spawnSync(
          process.execPath,
          [join(source, "hosts", `${target}.mjs`), configFile],
          { input: JSON.stringify(request), encoding: "utf8" },
        );
        const rejected = child.status !== 0;
        const expected =
          ["private_pure", "private_root", "wrong_name", "wrong_type"].includes(
            fault,
          ) ||
          (side === "new" &&
            target === "browser" &&
            ["missing_pure", "null_pure", "root_codec"].includes(fault));
        records.push({
          side,
          target,
          fault,
          exit: child.status,
          rejected,
          expected,
          stderr: child.stderr,
        });
        const label = `${side}/${target}/${fault}`;
        assert.equal(
          rejected,
          expected,
          `${label}: ${JSON.stringify(records)}`,
        );
        if (rejected) {
          const reason = ["wrong_name", "wrong_type"].includes(fault)
            ? "Unexpected installed SDK package identity"
            : fault === "private_root"
              ? "SDK entry does not identify the installed public root"
              : side === "old" || target === "node"
                ? "Codecs must use the installed public root"
                : "Browser codecs must use the installed public ./pure export";
          assert.ok(child.stderr.includes(reason), `${label}: ${child.stderr}`);
        }
        if (!rejected) assert.equal(JSON.parse(child.stdout).ready, true);
      }
    }
  }
  console.log(JSON.stringify(records, null, 2));
} finally {
  await rm(directory, { recursive: true, force: true });
}
