import assert from "node:assert/strict";

import { chromium } from "../../../node_modules/playwright/index.mjs";
import { createMigrationServer, withMigrationPage } from "./server.mjs";

const limit = 100_000_000;
const allocations = [];
const copies = [];
const server = await createMigrationServer(
  "crates/xmtp_legacy_migration/fixtures",
  ["stable"],
);
const browser = await chromium.launch({ headless: true });
try {
  await withMigrationPage(browser, server, async (page) => {
    page.on("console", (event) => {
      if (event.text().startsWith("MIGRATION_ARCHIVE_READ "))
        allocations.push(
          Number(event.text().slice("MIGRATION_ARCHIVE_READ ".length)),
        );
      if (event.text().startsWith("MIGRATION_ARCHIVE_COPY "))
        copies.push(
          Number(event.text().slice("MIGRATION_ARCHIVE_COPY ".length)),
        );
    });
    // verifies: MIG-008
    const result = await page.evaluate(async (limit) => {
      const { fixtureCall } = await import("./support.mjs");
      const fixture = new Uint8Array(
        await (await fetch("/row-fixture/stable")).arrayBuffer(),
      );
      const source = "/archive-read-limit.db3";
      await fixtureCall({ operation: "import", path: source, bytes: fixture });
      const NativeWorker = globalThis.Worker;
      globalThis.Worker = class extends NativeWorker {
        constructor(url, options) {
          super(
            new URL(url, location.href).pathname.endsWith(
              "/worker-entry.gen.js",
            )
              ? new URL("./read-limit-worker.mjs", location.href)
              : url,
            options,
          );
        }
      };
      try {
        const sdk = await import("/target/sdk-packages/browser/entry.js");
        const args = {
          databasePath: source,
          archiveKey: new Uint8Array(32).fill(7),
          outputPath: "read-limit.xmtp",
        };
        await sdk.prepareMigrationArchive(args);
        const original = await sdk.readMigrationArchive(args.outputPath);
        const root = await navigator.storage.getDirectory();
        const archives = await root.getDirectoryHandle(
          "xmtp-migration-archives",
        );
        const folder = await archives.getDirectoryHandle(
          encodeURIComponent(args.outputPath),
        );
        const [name] = await Array.fromAsync(folder.keys());
        const file = await folder.getFileHandle(name);
        async function resize(size) {
          const writable = await file.createWritable({
            keepExistingData: true,
          });
          await writable.truncate(size);
          await writable.close();
        }
        await resize(limit);
        let bytes = await sdk.readMigrationArchive(args.outputPath);
        const length = bytes.length;
        const header = original.every((byte, index) => bytes[index] === byte);
        const tail = bytes.at(-1);
        bytes = undefined;
        await resize(limit + 1);
        let failure;
        try {
          await sdk.readMigrationArchive(args.outputPath);
        } catch (error) {
          failure = error instanceof sdk.XmtpError.MigrationOutput;
        }
        const after = await file.getFile();
        const prefix = new Uint8Array(
          await after.slice(0, original.length).arrayBuffer(),
        );
        const preserved = original.every(
          (byte, index) => prefix[index] === byte,
        );
        const objects = await Array.fromAsync(folder.keys());
        await sdk.prepareMigrationArchive(args);
        const retry = await sdk.readMigrationArchive(args.outputPath);
        const sourceAfter = await fixtureCall({
          operation: "export",
          path: source,
        });
        return {
          length,
          header,
          tail,
          failure,
          size: after.size,
          preserved,
          objects,
          name,
          retry: retry.length > 0 && retry.length < limit,
          sourceUnchanged:
            sourceAfter.length === fixture.length &&
            sourceAfter.every((byte, index) => byte === fixture[index]),
        };
      } finally {
        globalThis.Worker = NativeWorker;
      }
    }, limit);
    assert.equal(
      result.length,
      limit,
      "100 MB archive was rejected or truncated",
    );
    assert.equal(result.header, true);
    assert.equal(result.tail, 0);
    assert.equal(
      result.failure,
      true,
      "oversized archive was not rejected with MigrationOutput",
    );
    assert.equal(result.size, limit + 1);
    assert.equal(result.preserved, true);
    assert.deepEqual(result.objects, [result.name]);
    assert.equal(result.retry, true);
    assert.equal(result.sourceUnchanged, true);
    assert.ok(allocations.includes(limit), "boundary read was not observed");
    assert.equal(
      copies.includes(limit),
      false,
      "archive read made a duplicate 100 MB byte-array copy",
    );
    assert.equal(
      allocations.includes(limit + 1),
      false,
      "oversized archive bytes were allocated before rejection",
    );
  });
  console.log(
    "Browser archive read: inclusive 100 MB limit, no duplicate copy, rejection before allocation, preservation, and retry passed",
  );
} finally {
  await browser.close();
  await server.close();
}
