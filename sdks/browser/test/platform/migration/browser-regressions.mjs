import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { chromium } from "../../../node_modules/playwright/index.mjs";
import { createServer } from "../../../node_modules/vite/dist/node/index.js";
const selected = process.argv[2] ?? "all";
const server = await createServer({
  root: process.cwd(),
  configFile: false,
  cacheDir: "target/migration-regression-vite",
  optimizeDeps: { noDiscovery: true },
  server: {
    hmr: false,
    watch: null,
    host: "127.0.0.1",
    port: 0,
    fs: { strict: false },
  },
});
await server.listen();
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage();
page.on("console", (event) => console.log("browser:", event.text()));
const source = "/migration-regression.db3";
const fixture = [
  ...(await readFile("crates/xmtp_legacy_migration/fixtures/stable.db3")),
];
try {
  await page.goto(
    `http://127.0.0.1:${server.httpServer.address().port}/sdks/browser/test/platform/migration/browser.html`,
  );
  await page.evaluate(
    async ({ source, fixture }) => {
      const worker = new Worker(
        new URL("./fixture-worker.mjs", location.href),
        { type: "module" },
      );
      try {
        await new Promise((resolve, reject) => {
          worker.onmessage = ({ data }) =>
            data.ok ? resolve() : reject(new Error(data.error));
          worker.onerror = (error) => reject(new Error(error.message));
          worker.postMessage({
            operation: "import",
            path: source,
            bytes: new Uint8Array(fixture),
          });
        });
      } finally {
        worker.terminate();
      }
    },
    { source, fixture },
  );
  if (["all", "memory"].includes(selected)) {
    const temporary = await mkdtemp(join(tmpdir(), "migration-memory-"));
    const memoryPage = await browser.newPage();
    try {
      const fixturePath = join(temporary, "large.db3");
      execFileSync("python3", [
        "-c",
        `
import random, shutil, sqlite3, sys
shutil.copyfile("crates/xmtp_legacy_migration/fixtures/stable.db3", sys.argv[1])
connection = sqlite3.connect(sys.argv[1])
row = list(connection.execute("SELECT * FROM group_messages WHERE kind = 1 AND expire_at_ns IS NULL LIMIT 1").fetchone())
random_bytes = random.Random(7)
for index in range(512):
    row[0] = (index + 1024).to_bytes(32, "big")
    row[2] = random_bytes.randbytes(8192)
    connection.execute("INSERT INTO group_messages VALUES (" + ",".join("?" for _ in row) + ")", row)
connection.commit()
connection.close()
`,
        fixturePath,
      ]);
      let measured;
      memoryPage.on("console", (event) => {
        if (event.text().startsWith("MIGRATION_OUTPUT_MEMORY "))
          measured = JSON.parse(
            event.text().slice("MIGRATION_OUTPUT_MEMORY ".length),
          );
      });
      await memoryPage.goto(page.url());
      const result = await memoryPage.evaluate(
        async (bytes) => {
          const fixture = new Uint8Array(bytes);
          const path = "/migration-many-frames.db3";
          const { fixtureCall } =
            await import("/sdks/browser/test/platform/migration/support.mjs");
          await fixtureCall({ operation: "import", path, bytes: fixture });
          const NativeWorker = globalThis.Worker;
          globalThis.Worker = class extends NativeWorker {
            constructor(url, options) {
              super(
                new URL(url, location.href).pathname.endsWith(
                  "/worker-entry.gen.js",
                )
                  ? new URL("./memory-worker.mjs", location.href)
                  : url,
                options,
              );
            }
          };
          try {
            const migration =
              await import("/target/sdk-packages/browser/entry.js");
            const report = await migration.prepareMigrationArchive({
              databasePath: path,
              archiveKey: new Uint8Array(32).fill(7),
              outputPath: "many-frames.xmtp",
            });
            const length = (
              await migration.readMigrationArchive(report.archivePath)
            ).length;
            globalThis.Worker = NativeWorker;
            const after = await fixtureCall({ operation: "export", path });
            return {
              messages: report.messageCount,
              length,
              sourceUnchanged:
                after.length === fixture.length &&
                after.every((byte, index) => byte === fixture[index]),
            };
          } finally {
            globalThis.Worker = NativeWorker;
          }
        },
        [...(await readFile(fixturePath))],
      );
      assert.equal(result.messages, 515n);
      assert.equal(result.sourceUnchanged, true);
      assert.ok(
        measured?.total > 4 * 1024 * 1024,
        "many-frame output was too small",
      );
      assert.equal(measured.total, result.length);
      assert.ok(
        measured.maximum <= 64 * 1024,
        `output retained a whole archive: ${JSON.stringify(measured)}`,
      );
      console.log(
        `Many-frame output: ${JSON.stringify(measured)}, source unchanged`,
      );
    } finally {
      await memoryPage.close();
      await rm(temporary, { recursive: true, force: true });
    }
  }
  if (["all", "busy"].includes(selected)) {
    const busy = await page.evaluate(async (source) => {
      const migration = await import("/target/sdk-packages/browser/entry.js");
      const storagePoolLock = "xmtp:.opfs-libxmtp-metadata";
      const owner = new Worker(
        new URL("./legacy-owner-worker.mjs", location.href),
        { type: "module" },
      );
      let tag;
      try {
        const count = await new Promise((resolve, reject) => {
          owner.onmessage = ({ data }) =>
            data.error ? reject(new Error(data.error)) : resolve(data.count);
          owner.onerror = (error) => reject(new Error(error.message));
          owner.postMessage({
            directory: storagePoolLock.slice("xmtp:".length).replace(/^\//, ""),
          });
        });
        if (count === 0) throw new Error("legacy owner held no OPFS handles");
        if ((await navigator.locks.query()).held.length)
          throw new Error("legacy owner unexpectedly uses Web Locks");
        try {
          await migration.prepareMigrationArchive({
            databasePath: source,
            archiveKey: new Uint8Array(32).fill(7),
            outputPath: "busy.xmtp",
          });
        } catch (error) {
          tag =
            error instanceof migration.XmtpError.StorageBusy
              ? "SourceBusy"
              : error.constructor.name;
        }
      } finally {
        owner.terminate();
      }
      return tag;
    }, source);
    assert.equal(
      busy,
      "SourceBusy",
      "legacy OPFS ownership must be retryable SourceBusy",
    );
    const retry = await page.evaluate(async (source) => {
      const migration = await import("/target/sdk-packages/browser/entry.js");
      return await migration.prepareMigrationArchive({
        databasePath: source,
        archiveKey: new Uint8Array(32).fill(7),
        outputPath: "busy.xmtp",
      });
    }, source);
    assert.equal(retry.messageCount, 3n);
    console.log(
      "Legacy owner without Web Locks: SourceBusy and immediate retry passed",
    );
  }
  if (["all", "recovery"].includes(selected)) {
    const recovered = await page.evaluate(async (source) => {
      const NativeWorker = globalThis.Worker;
      const workers = [];
      globalThis.Worker = class extends NativeWorker {
        constructor(url, options) {
          super(url, options);
          workers.push(this);
        }
      };
      try {
        const migration = await import("/target/sdk-packages/browser/entry.js");
        const admin = await migration.Storage.admin();
        const args = {
          databasePath: source,
          archiveKey: new Uint8Array(32).fill(7),
          outputPath: "recovered-worker.xmtp",
        };
        let ownerBusy;
        try {
          await migration.prepareMigrationArchive(args);
        } catch (error) {
          ownerBusy = error;
        }
        if (!(ownerBusy instanceof migration.XmtpError.StorageBusy))
          throw new Error("migration admitted a live storage owner");
        if (workers.length !== 1)
          throw new Error("unexpected initial worker count");
        workers[0].dispatchEvent(new Event("error"));
        await navigator.locks.request("xmtp:.opfs-libxmtp-metadata", () => {});
        const first = await migration.prepareMigrationArchive(args);
        workers[0].dispatchEvent(new Event("error"));
        const second = await migration.prepareMigrationArchive(args);
        const failedAdmin = await admin.fileCount().then(
          () => false,
          () => true,
        );
        if (!failedAdmin)
          throw new Error(
            "the old storage owner became usable after worker failure",
          );
        return {
          workers: workers.length,
          counts: [first.messageCount, second.messageCount],
        };
      } finally {
        globalThis.Worker = NativeWorker;
      }
    }, source);
    assert.deepEqual(recovered, { workers: 3, counts: [3n, 3n] });
    console.log(
      "Failed normal worker: live owner rejection, direct migration retry, fresh workers, and late error passed",
    );
  }
  if (["all", "publication"].includes(selected)) {
    for (const phase of ["before-close", "after-close"]) {
      for (const existing of [false, true]) {
        const path = `interrupted-${phase}-${existing}.xmtp`;
        const before = existing
          ? await page.evaluate(
              async ({ source, path }) => {
                const migration =
                  await import("/target/sdk-packages/browser/entry.js");
                await migration.prepareMigrationArchive({
                  databasePath: source,
                  archiveKey: new Uint8Array(32).fill(7),
                  outputPath: path,
                });
                return [...(await migration.readMigrationArchive(path))];
              },
              { source, path },
            )
          : undefined;
        const paused = page.waitForEvent("console", {
          predicate: (event) => event.text() === "MIGRATION_PUBLICATION_PAUSED",
          timeout: 20000,
        });
        const pending = page
          .evaluate(
            async ({ source, path, phase }) => {
              const NativeWorker = globalThis.Worker;
              globalThis.Worker = class extends NativeWorker {
                constructor(url, options) {
                  super(
                    new URL(url, location.href).pathname.endsWith(
                      "/worker-entry.gen.js",
                    )
                      ? new URL(
                          `./interruption-worker.mjs?phase=${phase}`,
                          location.href,
                        )
                      : url,
                    options,
                  );
                }
              };
              const migration =
                await import("/target/sdk-packages/browser/entry.js");
              await migration.prepareMigrationArchive({
                databasePath: source,
                archiveKey: new Uint8Array(32).fill(7),
                outputPath: path,
              });
            },
            { source, path, phase },
          )
          .catch((error) => String(error));
        await paused;
        await page.reload();
        await pending;
        const recovered = await page.evaluate(
          async ({ source, path }) => {
            const migration =
              await import("/target/sdk-packages/browser/entry.js");
            let bytes;
            try {
              bytes = [...(await migration.readMigrationArchive(path))];
            } catch (error) {
              if (!(error instanceof migration.XmtpError.MigrationOutput))
                throw error;
            }
            const root = await navigator.storage.getDirectory();
            const archives = await root.getDirectoryHandle(
              "xmtp-migration-archives",
            );
            const folder = await archives.getDirectoryHandle(
              encodeURIComponent(path),
            );
            const count = (await Array.fromAsync(folder.keys())).length;
            const report = await migration.prepareMigrationArchive({
              databasePath: source,
              archiveKey: new Uint8Array(32).fill(7),
              outputPath: path,
            });
            return { bytes, count, messages: report.messageCount };
          },
          { source, path },
        );
        assert.deepEqual(
          recovered.bytes,
          before,
          "interruption published or replaced incomplete output",
        );
        assert.equal(
          recovered.count,
          existing ? 1 : 0,
          "next access did not reclaim unpublished output",
        );
        assert.equal(recovered.messages, 3n);
      }
    }
    console.log(
      "Page termination before/after object close: new/prior output, recovery cleanup, and retry passed",
    );
  }
  const after = await page.evaluate(async (source) => {
    const worker = new Worker(new URL("./fixture-worker.mjs", location.href), {
      type: "module",
    });
    try {
      return await new Promise((resolve, reject) => {
        worker.onmessage = ({ data }) =>
          data.ok ? resolve([...data.value]) : reject(new Error(data.error));
        worker.postMessage({ operation: "export", path: source });
      });
    } finally {
      worker.terminate();
    }
  }, source);
  assert.deepEqual(
    after,
    fixture,
    "source bytes changed during browser failure or recovery",
  );
} finally {
  await browser.close();
  await server.close();
}
