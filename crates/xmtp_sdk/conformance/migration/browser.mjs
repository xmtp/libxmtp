import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

import { chromium } from "../../../../sdks/browser/node_modules/playwright/index.mjs";
import { createServer } from "../../../../sdks/browser/node_modules/vite/dist/node/index.js";
const server = await createServer({
  root: process.cwd(),
  configFile: false,
  cacheDir: "target/migration-browser-vite",
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
const page = await browser.newPage();
page.on("console", (event) => console.log("browser:", event.text()));
page.on("pageerror", (error) => console.error(error));
try {
  await page.goto(
    `http://127.0.0.1:${server.httpServer.address().port}/crates/xmtp_sdk/conformance/migration/browser.html`,
  );
  const fixture = [
    ...(await readFile(
      "crates/xmtp_legacy_migration/fixtures/consent-states.db3",
    )),
  ];
  const result = await page.evaluate(
    async ({ fixture, currentSdk }) => {
      const migration = await import("/target/sdk-packages/browser/entry.js");
      const storagePoolLock = "xmtp:.opfs-libxmtp-metadata";
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
          report.consentCount === 3n,
        "report counts lost their bigint values",
      );
      const archive = await migration.readMigrationArchive(report.archivePath);
      const records = JSON.parse(
        await fixtureCall({ operation: "inspect", bytes: archive, key }),
      );
      check(records.length === 8, "wrong archive element count");
      const consents = records.filter((record) => record.kind === "consent");
      check(
        consents.length === 3 &&
          consents.every(
            (record, index) =>
              record.entity ===
                (index + 2).toString(16).padStart(2, "0").repeat(32) &&
              record.state === index + 1 &&
              record.consentedAtNs === "1700000000000000009",
          ),
        "legacy consent states changed in the archive",
      );
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
        busy instanceof migration.XmtpError.StorageBusy,
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
        invalid instanceof migration.XmtpError.InvalidInput,
        "invalid key lost its typed tag",
      );
      const NativeWorker = globalThis.Worker;
      let outputFailure;
      globalThis.Worker = class extends NativeWorker {
        constructor(url, options) {
          super(
            new URL(url, location.href).pathname.endsWith(
              "/worker-entry.gen.js",
            )
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
              outputFailure instanceof migration.XmtpError.MigrationOutput,
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
        absent = error instanceof migration.XmtpError.MigrationOutput;
      }
      check(absent, "failed write published a new partial output");
      const repeated = await migration.prepareMigrationArchive(args);
      check(
        repeated.messageCount === 3n,
        "immediate worker replacement failed",
      );
      if (currentSdk) {
        const sdk = migration;
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
          let ownerBusy;
          try {
            await migration.prepareMigrationArchive(args);
          } catch (error) {
            ownerBusy = error;
          }
          check(
            ownerBusy instanceof sdk.XmtpError.StorageBusy,
            "migration admitted an active package client",
          );
          const latest = await migration.readMigrationArchive(
            repeated.archivePath,
          );
          await client.archives.importFromBytes(latest, key);
          await client.archives.importFromBytes(latest, key);
          for (const [prefix, state] of [
            ["02", "unknown"],
            ["03", "allowed"],
            ["04", "denied"],
          ]) {
            check(
              (await client.preferences.consentState({
                kind: "inbox",
                inboxId: prefix.repeat(32),
              })) === state,
              `imported consent state changed for ${prefix}`,
            );
          }
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
  assert.deepEqual(result.counts, ["2", "3", "3"]);
  console.log(
    "Browser package: OPFS worker export, exact source name, source preservation, standard archive decoding, bigint precision, metadata, all consent states, typed errors, and immediate worker replacement passed",
  );
} finally {
  await browser.close();
  await server.close();
}
