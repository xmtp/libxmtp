import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { chromium } from "../../../node_modules/playwright/index.mjs";
import { createMigrationServer, withMigrationPage } from "./server.mjs";
const temporary = await mkdtemp(join(tmpdir(), "migration-copy-"));
const sourceBytes = 128 * 1024 * 1024;
execFileSync("python3", [
  "-c",
  "import shutil,sqlite3,sys; p=sys.argv[1]+'/large.db3';shutil.copyfile('crates/xmtp_legacy_migration/fixtures/stable.db3',p);c=sqlite3.connect(p);c.execute('INSERT INTO openmls_key_value(key_bytes,value_bytes,version) VALUES (?,zeroblob(?),1)',(b'unrelated-secret',int(sys.argv[2])));c.commit();c.close()",
  temporary,
  String(sourceBytes),
]);
const server = await createMigrationServer(temporary, ["large"]);
const browser = await chromium.launch({ headless: true });
try {
  await withMigrationPage(browser, server, async (page) => {
    const metrics = [];
    page.on("console", (event) => {
      if (event.text().startsWith("MIGRATION_SOURCE_IO "))
        metrics.push(
          JSON.parse(event.text().slice("MIGRATION_SOURCE_IO ".length)),
        );
    });
    const before = await page.evaluate(async () => {
      const { fixtureCall } = await import("./support.mjs");
      let bytes = new Uint8Array(
        await (await fetch("/row-fixture/large")).arrayBuffer(),
      );
      await fixtureCall({ operation: "import", path: "/large.db3", bytes });
      bytes = undefined;
      const source = await fixtureCall({
        operation: "export",
        path: "/large.db3",
      });
      return Array.from(
        new Uint8Array(await crypto.subtle.digest("SHA-256", source)),
      );
    });
    async function prepare(
      phase,
      outputPath = "working.xmtp",
      detached = false,
    ) {
      return page.evaluate(
        async ({ phase, outputPath, detached }) => {
          const sdk = await import("/target/sdk-packages/browser/entry.js");
          if (!globalThis.OriginalWorker)
            globalThis.OriginalWorker = globalThis.Worker;
          globalThis.Worker = class extends globalThis.OriginalWorker {
            constructor(url, options) {
              super(
                new URL(url, location.href).pathname.endsWith(
                  "/worker-entry.gen.js",
                )
                  ? new URL("./copy-worker.mjs?phase=" + phase, location.href)
                  : url,
                options,
              );
              if (phase === "interrupt") globalThis.interruptedWorker = this;
            }
            postMessage(message, transfer) {
              if (message.lifetimeLock)
                this.lifetimeLock = message.lifetimeLock;
              return super.postMessage(message, transfer);
            }
          };
          const operation = sdk.prepareMigrationArchive({
            databasePath: "/large.db3",
            archiveKey: new Uint8Array(32).fill(7),
            outputPath,
          });
          if (detached) {
            operation.catch(() => {});
            return;
          }
          try {
            const report = await operation;
            return { messages: String(report.messageCount) };
          } catch (error) {
            return {
              outputFailure: error instanceof sdk.XmtpError.MigrationOutput,
              invalidInput: error instanceof sdk.XmtpError.InvalidInput,
              error: String(error),
            };
          }
        },
        { phase, outputPath, detached },
      );
    }
    assert.deepEqual(await prepare("measure"), { messages: "3" });
    assert.ok(
      metrics.some((m) => m.total >= sourceBytes),
      "source copy was not observed",
    );
    assert.ok(
      metrics.every((m) => m.maximum <= 64 * 1024),
      "source read exceeded 64 KiB",
    );
    assert.equal(
      (await prepare("permission")).invalidInput,
      true,
      "source permission failure was classified as contention",
    );
    const setupIo = await prepare("setup-io");
    assert.equal(
      setupIo.outputFailure,
      true,
      `private-copy setup I/O was classified as input: ${JSON.stringify(setupIo)}`,
    );
    const completed = await page.evaluate(async () =>
      Array.from(
        await (
          await import("/target/sdk-packages/browser/entry.js")
        ).readMigrationArchive("working.xmtp"),
      ),
    );
    assert.equal(
      (await prepare("quota")).outputFailure,
      true,
      "copy quota did not retain Output class",
    );
    await page.evaluate(async (completed) => {
      const sdk = await import("/target/sdk-packages/browser/entry.js");
      const bytes = await sdk.readMigrationArchive("working.xmtp");
      if (
        bytes.length !== completed.length ||
        !bytes.every((byte, i) => byte === completed[i])
      )
        throw Error("prior output changed during copy failure");
    }, completed);
    assert.deepEqual(await prepare("measure"), { messages: "3" });
    const pause = page.waitForEvent("console", {
      predicate: (event) => event.text() === "MIGRATION_COPY_PAUSED",
      timeout: 20000,
    });
    await prepare("interrupt", "new-working.xmtp", true);
    await pause;
    await page.evaluate(async () => {
      const worker = globalThis.interruptedWorker;
      worker.terminate();
      await navigator.locks.request(worker.lifetimeLock, () => {});
    });
    await page.reload();
    await page.evaluate(async () => {
      const sdk = await import("/target/sdk-packages/browser/entry.js");
      try {
        await sdk.readMigrationArchive("new-working.xmtp");
        throw Error("copy interruption published an archive");
      } catch (error) {
        if (!(error instanceof sdk.XmtpError.MigrationOutput)) throw error;
      }
    });
    assert.deepEqual(await prepare("measure"), { messages: "3" });
    const after = await page.evaluate(async () => {
      const { fixtureCall } = await import("./support.mjs");
      const source = await fixtureCall({
        operation: "export",
        path: "/large.db3",
      });
      return Array.from(
        new Uint8Array(await crypto.subtle.digest("SHA-256", source)),
      );
    });
    assert.deepEqual(
      after,
      before,
      "source bytes changed during working-copy paths",
    );
    const files = await page.evaluate(async () => {
      const root = await navigator.storage.getDirectory();
      const work = await root.getDirectoryHandle(".xmtp-migration-working");
      const sizes = [];
      async function inspect(directory) {
        for await (const item of directory.values()) {
          if (item.kind === "directory") await inspect(item);
          else sizes.push((await item.getFile()).size);
        }
      }
      await inspect(work);
      return sizes;
    });
    assert.ok(
      files.length > 0 && files.every((size) => size <= 4096),
      "private working data remains after cleanup",
    );
    await page.evaluate(async () => {
      const { fixtureCall, check } = await import("./support.mjs");
      const sdk = await import("/target/sdk-packages/browser/entry.js");
      const bytes = new Uint8Array(
        await (
          await fetch("/crates/xmtp_legacy_migration/fixtures/stable.db3")
        ).arrayBuffer(),
      );
      for (const suffix of ["-journal", "-wal", "-shm"]) {
        const path = "/source" + suffix + ".db3";
        await fixtureCall({ operation: "import", path, bytes });
        if (suffix === "-journal") {
          await fixtureCall({ operation: "hot-journal", path });
        } else {
          // Any sidecar requires recovery, including malformed WAL or SHM data.
          await fixtureCall({
            operation: "import",
            path: path + suffix,
            bytes,
          });
        }
        const before = await fixtureCall({ operation: "export", path });
        const sidecar = await fixtureCall({
          operation: "export",
          path: path + suffix,
        });
        if (suffix === "-journal")
          check(
            [0xd9, 0xd5, 0x05, 0xf9, 0x20, 0xa1, 0x63, 0xd7].every(
              (byte, i) => sidecar[i] === byte,
            ),
            "fixture has no hot SQLite journal",
          );
        const previous = await sdk.readMigrationArchive("working.xmtp");
        const newOutput = "new" + suffix + ".xmtp";
        for (const outputPath of ["working.xmtp", newOutput]) {
          let rejected = false;
          try {
            await sdk.prepareMigrationArchive({
              databasePath: path,
              archiveKey: new Uint8Array(32).fill(7),
              outputPath,
            });
          } catch (error) {
            rejected = error instanceof sdk.XmtpError.InvalidInput;
          }
          check(
            rejected,
            "source sidecar was not rejected as InvalidInput: " + suffix,
          );
        }
        const after = await fixtureCall({ operation: "export", path });
        const afterSidecar = await fixtureCall({
          operation: "export",
          path: path + suffix,
        });
        check(
          before.length === after.length &&
            before.every((byte, i) => after[i] === byte),
          "source was changed",
        );
        check(
          sidecar.length === afterSidecar.length &&
            sidecar.every((byte, i) => afterSidecar[i] === byte),
          "source sidecar was changed",
        );
        const preserved = await sdk.readMigrationArchive("working.xmtp");
        check(
          previous.length === preserved.length &&
            previous.every((byte, i) => preserved[i] === byte),
          "sidecar rejection replaced prior output",
        );
        let absent = false;
        try {
          await sdk.readMigrationArchive(newOutput);
        } catch (error) {
          absent = error instanceof sdk.XmtpError.MigrationOutput;
        }
        check(absent, "sidecar rejection published new output");
      }
    });
    console.log(
      "OPFS copy: bounded reads, 128 MiB source, quota, interruption, recovery, cleanup, sidecar rejection, and source preservation passed",
    );
  });
} finally {
  await browser.close();
  await server.close();
  await rm(temporary, { recursive: true, force: true });
}
