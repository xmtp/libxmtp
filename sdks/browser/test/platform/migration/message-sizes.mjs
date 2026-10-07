import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createReadStream } from "node:fs";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { chromium } from "../../../node_modules/playwright/index.mjs";
import { createServer } from "../../../node_modules/vite/dist/node/index.js";

const limit = 64 * 1024 * 1024;
const temporary = await mkdtemp(join(tmpdir(), "migration-row-size-"));
const cases = ["single", "combined", "unicode", "boundary"];
execFileSync("python3", [
  "-c",
  `
import shutil, sqlite3, sys
from pathlib import Path
limit=64*1024*1024
for name,content,reference,inbox in [('single',limit+1,1,'é'),('combined',limit//2,limit//2,'é'),('unicode',limit-54,1,'éé'),('boundary',limit-53,1,'é')]:
    path=Path(sys.argv[1])/(name+'.db3')
    shutil.copyfile('crates/xmtp_legacy_migration/fixtures/stable.db3',path)
    connection=sqlite3.connect(path)
    connection.execute("UPDATE group_messages SET decrypted_message_bytes=zeroblob(?), sender_installation_id=x'01', sender_inbox_id=?, authority_id='a', reference_id=zeroblob(?) WHERE id=?",(content,inbox,reference,bytes([1])*32))
    connection.commit()
    connection.close()
`,
  temporary,
]);
const server = await createServer({
  plugins: [
    {
      name: "migration-row-fixtures",
      configureServer(server) {
        server.middlewares.use((request, response, next) => {
          const name = request.url?.replace(/^\/row-fixture\//, "");
          if (!cases.includes(name)) return next();
          response.setHeader("Content-Type", "application/octet-stream");
          createReadStream(join(temporary, name + ".db3")).pipe(response);
        });
      },
    },
  ],
  root: process.cwd(),
  configFile: false,
  cacheDir: "target/migration-message-size-vite",
  optimizeDeps: { noDiscovery: true },
  server: {
    hmr: false,
    watch: null,
    host: "127.0.0.1",
    port: 0,
    fs: { strict: false },
    proxy: {
      "/backend": {
        target: process.env.XMTP_BACKEND_URL ?? "http://127.0.0.1:5050",
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/backend/, ""),
      },
    },
  },
});
await server.listen();
const browser = await chromium.launch({ headless: true });
try {
  for (const name of cases) {
    const context = await browser.newContext();
    const page = await context.newPage();
    try {
      await page.goto(
        `http://127.0.0.1:${server.httpServer.address().port}/sdks/browser/test/platform/migration/browser.html`,
      );
      const result = await page.evaluate(
        async ({ name, limit }) => {
          const sdk = await import("/target/sdk-packages/browser/entry.js");
          function check(value, message) {
            if (!value) throw new Error(message);
          }
          async function fixtureCall(data) {
            const worker = new Worker(
              new URL("./fixture-worker.mjs", location.href),
              { type: "module" },
            );
            try {
              return await new Promise((resolve, reject) => {
                worker.onmessage = ({ data }) =>
                  data.ok ? resolve(data.value) : reject(Error(data.error));
                worker.onerror = (error) => reject(Error(error.message));
                worker.postMessage(data);
              });
            } finally {
              worker.terminate();
              await new Promise((resolve) => setTimeout(resolve, 25));
            }
          }
          const source = "/row-size.db3";
          let fixture = new Uint8Array(
            await (await fetch("/row-fixture/" + name)).arrayBuffer(),
          );
          check(
            new TextDecoder().decode(fixture.subarray(0, 15)) ===
              "SQLite format 3",
            "fixture route did not return SQLite bytes",
          );
          await fixtureCall({
            operation: "import",
            path: source,
            bytes: fixture,
          });
          fixture = undefined;
          const before = await fixtureCall({
            operation: "export",
            path: source,
          });
          const key = new Uint8Array(32).fill(7);
          const args = {
            databasePath: source,
            archiveKey: key,
            outputPath: "row-size.xmtp",
          };
          if (name !== "boundary") {
            const baseline = new Uint8Array(
              await (
                await fetch("/crates/xmtp_legacy_migration/fixtures/stable.db3")
              ).arrayBuffer(),
            );
            await fixtureCall({
              operation: "import",
              path: "/small.db3",
              bytes: baseline,
            });
            await sdk.prepareMigrationArchive({
              ...args,
              databasePath: "/small.db3",
            });
            const completed = await sdk.readMigrationArchive(args.outputPath);
            for (const previous of [false, true]) {
              const outputPath = previous
                ? args.outputPath
                : "new-row-size.xmtp";
              let failure;
              try {
                await sdk.prepareMigrationArchive({ ...args, outputPath });
              } catch (error) {
                failure = error;
              }
              check(
                failure instanceof sdk.XmtpError.MigrationRecordRead,
                name +
                  ": oversized row did not return MigrationRecordRead (" +
                  (failure ? String(failure) : "preparation succeeded") +
                  ")",
              );
              // Inspect before another archive call can perform recovery.
              const root = await navigator.storage.getDirectory();
              const archives = await root.getDirectoryHandle(
                "xmtp-migration-archives",
              );
              const folder = await archives.getDirectoryHandle(
                encodeURIComponent(outputPath),
              );
              check(
                (await Array.fromAsync(folder.keys())).length ===
                  (previous ? 1 : 0),
                "private output remains after failure",
              );
              if (previous) {
                const preserved = await sdk.readMigrationArchive(outputPath);
                check(
                  preserved.length === completed.length &&
                    preserved.every((byte, i) => byte === completed[i]),
                  "completed output changed",
                );
              } else {
                let absent = false;
                try {
                  await sdk.readMigrationArchive(outputPath);
                } catch (error) {
                  absent = error instanceof sdk.XmtpError.MigrationOutput;
                }
                check(absent, "failed preparation published a new output");
              }
            }
            const retry = await sdk.prepareMigrationArchive({
              ...args,
              databasePath: "/small.db3",
            });
            check(
              retry.messageCount === 3n,
              "failure did not release storage for retry",
            );
          } else {
            for (let attempt = 0; attempt < 2; attempt++) {
              const report = await sdk.prepareMigrationArchive(args);
              check(
                report.groupCount === 2n &&
                  report.messageCount === 3n &&
                  report.consentCount === 1n,
                "boundary row was omitted",
              );
            }
            const archive = await sdk.readMigrationArchive(args.outputPath);
            const records = JSON.parse(
              await fixtureCall({ operation: "sizes", bytes: archive, key }),
            );
            const message = records.find(
              (record) => record.id === "01".repeat(32),
            );
            check(
              records.length === 3 &&
                message.contentLength === limit - 53 &&
                message.contentIsZero,
              "boundary content changed",
            );
            check(
              message.installationLength === 1 &&
                message.senderInboxId === "é" &&
                message.authorityId === "a" &&
                message.referenceLength === 1,
              "boundary fields changed",
            );
            const { generatePrivateKey } =
              await import("/sdks/browser/node_modules/viem/_esm/accounts/generatePrivateKey.js");
            const { privateKeyToAccount } =
              await import("/sdks/browser/node_modules/viem/_esm/accounts/privateKeyToAccount.js");
            const { toBytes } =
              await import("/sdks/browser/node_modules/viem/_esm/utils/encoding/toBytes.js");
            const account = privateKeyToAccount(generatePrivateKey());
            const signer = {
              async identity() {
                return {
                  kind: "ethereum",
                  identifier: account.address.toLowerCase(),
                };
              },
              async kind() {
                return { kind: "eoa" };
              },
              async sign(request) {
                return {
                  kind: "ecdsa",
                  value: toBytes(
                    await account.signMessage({ message: request.text }),
                  ),
                };
              },
            };
            const client = await sdk.Client.create(signer, {
              backend: { url: `${location.origin}/backend` },
              storage: {
                location: {
                  dbPath: "/row-size-import.db3",
                  attachmentsDir: "row-size-attachments",
                },
              },
              deviceSync: false,
            });
            try {
              await client.archives.importFromBytes(archive, key);
              await client.archives.importFromBytes(archive, key);
              check(
                (
                  await client.conversations.listDms({
                    includeDuplicateDms: true,
                  })
                ).length === 1,
                "repeat import lost the DM",
              );
              check(
                (await client.conversations.listGroups(undefined)).length === 1,
                "repeat import changed groups",
              );
            } finally {
              await client.end();
            }
          }
          const after = await fixtureCall({
            operation: "export",
            path: source,
          });
          check(
            before.length === after.length &&
              before.every((byte, index) => byte === after[index]),
            "source bytes changed",
          );
          return { name, sourcePreserved: true };
        },
        { name, limit },
      );
      assert.equal(result.sourcePreserved, true);
      console.log(
        `${name}: 64 MiB row budget, source/output preservation, cleanup, and retry passed`,
      );
    } finally {
      await context.close();
    }
  }
} finally {
  await browser.close();
  await server.close();
  await rm(temporary, { recursive: true, force: true });
}
