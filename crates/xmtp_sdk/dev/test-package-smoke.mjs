import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
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
import { fileURLToPath } from "node:url";

const script = fileURLToPath(new URL("./package-smoke.mjs", import.meta.url));

for (const retained of [false, true]) {
  test(`installed smoke uses ${retained ? "the retained archive" : "the staged package"}`, () => {
    const root = mkdtempSync(join(tmpdir(), "sdk-smoke-archive-"));
    try {
      const stage = join(root, "stage/node");
      const manifest = {
        name: "@xmtp/node-sdk",
        version: "8.0.0",
        type: "module",
        engines: { node: ">=22.12.0" },
        exports: { ".": "./index.js" },
      };
      mkdirSync(stage, { recursive: true });
      writeFileSync(join(stage, "package.json"), JSON.stringify(manifest));
      const archives = [];
      for (const valid of [true, false]) {
        const directory = join(root, valid ? "valid" : "missing-entry");
        mkdirSync(join(directory, "package"), { recursive: true });
        writeFileSync(
          join(directory, "package/package.json"),
          JSON.stringify(manifest),
        );
        writeFileSync(join(directory, "package/sdk-contract.json"), "{}");
        if (valid)
          writeFileSync(
            join(directory, "package/index.js"),
            "export const sdkVersion=()=> '8.0.0'; export class TextCodec { encode(value) { return value; } decode(value) { return value; } }\n",
          );
        const archive = `${directory}.tgz`;
        execFileSync("tar", ["-czf", archive, "-C", directory, "package"]);
        archives.push(archive);
      }
      const npm = join(root, "npm-cli.js");
      const log = join(root, "calls.jsonl");
      writeFileSync(
        npm,
        `
const fs=require('node:fs'), path=require('node:path'), cp=require('node:child_process');
const args=process.argv.slice(2);
fs.appendFileSync(process.env.NPM_LOG,JSON.stringify(args)+'\\n');
if(args[0]==='pack') {
 const dest=args[args.indexOf('--pack-destination')+1];
 fs.copyFileSync(process.env.STAGED_ARCHIVE,path.join(dest,'staged.tgz'));
 console.log(JSON.stringify([{filename:'staged.tgz'}]));
} else if(args[0]==='install') {
 const dest=path.join(process.cwd(),'node_modules/@xmtp/node-sdk');
 fs.mkdirSync(dest,{recursive:true});
 cp.execFileSync('tar',['-xzf',args.at(-1),'-C',dest,'--strip-components=1']);
} else throw Error('unexpected npm command');
`,
      );
      for (const [index, archive] of archives.entries()) {
        if (!retained && index > 0) continue;
        writeFileSync(log, "");
        const result = spawnSync(
          process.execPath,
          [script, "node", ...(retained ? [archive] : [])],
          {
            env: {
              ...process.env,
              XMTP_SDK_PACKAGES_DIR: join(root, "stage"),
              XMTP_SDK_NPM_CLI: npm,
              NPM_LOG: log,
              STAGED_ARCHIVE: archives[0],
            },
            encoding: "utf8",
          },
        );
        assert.equal(result.status === 0, index === 0, result.stderr);
        const calls = readFileSync(log, "utf8")
          .trim()
          .split("\n")
          .map(JSON.parse);
        assert.equal(
          calls.some(([command]) => command === "pack"),
          !retained,
        );
        if (retained)
          assert.equal(
            calls.find(([command]) => command === "install").at(-1),
            archive,
          );
      }
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
}
