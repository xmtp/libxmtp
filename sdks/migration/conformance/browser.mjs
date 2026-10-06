import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

import { chromium } from "../../browser/node_modules/playwright/index.mjs";
import { createServer } from "../../browser/node_modules/vite/dist/node/index.js";
const server = await createServer({
  root: process.cwd(),
  configFile: false,
  cacheDir: "target/migration-browser-vite",
  optimizeDeps: { noDiscovery: true },
  server: {
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
const page = await browser.newPage();
page.on("console", (event) => console.log("browser:", event.text()));
page.on("pageerror", (error) => console.error(error));
try {
  await page.goto(
    `http://127.0.0.1:${server.httpServer.address().port}/sdks/migration/conformance/browser.html`,
  );
  const fixture = [
    ...(await readFile("crates/xmtp_legacy_migration/fixtures/stable.db3")),
  ];
  const result = await page.evaluate(
    async ({ fixture, currentSdk }) => {
      const migration =
        await import("/target/migration-packages/browser/index.js");
      const { storagePoolLock } =
        await import("/target/migration-packages/browser/generated/storage-pool.gen.js");
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
              data.ok ? resolve(data.value) : reject(new Error(data.error));
            worker.onerror = (e) => reject(new Error(e.message));
            worker.postMessage(data);
          });
        } finally {
          worker.terminate();
          // A new worker proves that the old OPFS handles are no longer held.
          await new Promise((resolve) => setTimeout(resolve, 25));
        }
      }
      const source = "/migration-leading-slash.db3";
      await fixtureCall({
        operation: "import",
        path: source,
        bytes: new Uint8Array(fixture),
      });
      const before = await fixtureCall({ operation: "export", path: source });
      const backing = new Uint8Array(100).fill(55);
      const key = backing.subarray(19, 51);
      key.fill(7);
      const args = {
        databasePath: source,
        archiveKey: key,
        outputPath: "migration-history.xmtp",
      };
      const report = await migration.prepareMigrationArchive(args);
      check(
        report.groupCount === 2n &&
          report.messageCount === 3n &&
          report.consentCount === 1n,
        "report counts lost their bigint values",
      );
      const archive = await migration.readMigrationArchive(report.archivePath);
      const records = JSON.parse(
        await fixtureCall({ operation: "inspect", bytes: archive, key }),
      );
      check(records.length === 6, "wrong archive element count");
      const messages = records.filter((record) => record.kind === "message");
      check(
        messages.length === 3 &&
          messages.every(
            (m) =>
              m.sentAtNs ===
              (m.id === "0a".repeat(32)
                ? "1500000000000000000"
                : "1700000000000000123"),
          ),
        "nanosecond precision or expiry eligibility changed",
      );
      check(
        records.find((r) => r.name === "Migration DM").metadata ===
          "01".repeat(32),
        "legacy metadata was lost",
      );
      const after = await fixtureCall({ operation: "export", path: source });
      check(
        before.length === after.length &&
          before.every((byte, index) => byte === after[index]),
        "source bytes changed",
      );
      let busy;
      await navigator.locks.request(storagePoolLock, async () => {
        try {
          await migration.prepareMigrationArchive(args);
        } catch (error) {
          busy = error;
        }
      });
      check(
        migration.MigrationError.SourceBusy.instanceOf(busy),
        "ownership error lost its typed tag",
      );
      let invalid;
      try {
        await migration.prepareMigrationArchive({
          ...args,
          archiveKey: new Uint8Array(31),
        });
      } catch (error) {
        invalid = error;
      }
      check(
        migration.MigrationError.InvalidInput.instanceOf(invalid),
        "invalid key lost its typed tag",
      );
      const NativeWorker = globalThis.Worker;
      let outputFailure;
      globalThis.Worker = class extends NativeWorker {
        constructor(url, options) {
          super(
            new URL(url, location.href).pathname.endsWith("/worker.js")
              ? new URL("./quota-worker.mjs", location.href)
              : url,
            options,
          );
        }
      };
      try {
        for (const outputPath of [args.outputPath, "failed-new-output.xmtp"]) {
          outputFailure = undefined;
          try {
            await migration.prepareMigrationArchive({ ...args, outputPath });
          } catch (error) {
            outputFailure = error;
          }
          check(
            outputFailure !== undefined &&
              migration.MigrationError.Output.instanceOf(outputFailure),
            "OPFS write failure lost its typed tag",
          );
        }
      } finally {
        globalThis.Worker = NativeWorker;
      }
      const preserved = await migration.readMigrationArchive(
        report.archivePath,
      );
      check(
        archive.length === preserved.length &&
          archive.every((byte, index) => byte === preserved[index]),
        "failed write replaced completed output",
      );
      let absent = false;
      try {
        await migration.readMigrationArchive("failed-new-output.xmtp");
      } catch (error) {
        absent =
          error instanceof DOMException && error.name === "NotFoundError";
      }
      check(absent, "failed write published a new partial output");
      const repeated = await migration.prepareMigrationArchive(args);
      check(
        repeated.messageCount === 3n,
        "immediate worker replacement failed",
      );
      // Another owner can acquire the pool after this worker releases it.
      // Cleanup must wait for its own worker, not that new owner.
      const query = navigator.locks.query;
      let releaseForeign;
      let foreignAcquired;
      const foreignGate = new Promise((resolve) => {
        releaseForeign = resolve;
      });
      const foreignReady = new Promise((resolve) => {
        foreignAcquired = resolve;
      });
      let foreignRequest;
      navigator.locks.query = async function () {
        const state = await query.call(this);
        if (
          !foreignRequest &&
          !state.held?.some((held) => held.name === storagePoolLock)
        ) {
          foreignRequest = navigator.locks.request(
            storagePoolLock,
            async () => {
              foreignAcquired();
              await foreignGate;
            },
          );
          await foreignReady;
          return query.call(this);
        }
        return state;
      };
      try {
        const completed = await migration.prepareMigrationArchive(args);
        check(
          foreignRequest !== undefined && completed.messageCount === 3n,
          "cleanup waited for another pool owner",
        );
      } finally {
        navigator.locks.query = query;
        releaseForeign();
        await foreignRequest;
      }
      if (currentSdk) {
        const sdk = await import("/sdks/browser/dist/entry.js");
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
              dbPath: "/migration-current-destination.db3",
              attachmentsDir: "migration-attachments",
            },
          },
          deviceSync: false,
        });
        try {
          const latest = await migration.readMigrationArchive(
            repeated.archivePath,
          );
          await client.archives.importFromBytes(latest, key);
          await client.archives.importFromBytes(latest, key);
          const dms = await client.conversations.listDms({
            includeDuplicateDms: true,
          });
          const groups = await client.conversations.listGroups(undefined);
          check(
            dms.length === 1 && groups.length === 1,
            "current browser SDK did not restore both conversations",
          );
        } finally {
          await client.end();
        }
      }
      return {
        records,
        counts: [
          report.groupCount.toString(),
          report.messageCount.toString(),
          report.consentCount.toString(),
        ],
      };
    },
    { fixture, currentSdk: process.argv.includes("--current-sdk") },
  );
  assert.deepEqual(result.counts, ["2", "3", "1"]);
  console.log(
    "Browser package: OPFS worker export, exact source name, source preservation, standard archive decoding, bigint precision, metadata, typed errors, and immediate worker replacement passed",
  );
} finally {
  await browser.close();
  await server.close();
}
