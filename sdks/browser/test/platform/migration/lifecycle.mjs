import assert from "node:assert/strict";

import { chromium } from "../../../node_modules/playwright/index.mjs";
import { createMigrationServer, withMigrationPage } from "./server.mjs";
const selected = process.argv[2] ?? "all";
const server = await createMigrationServer(
  "crates/xmtp_legacy_migration/fixtures",
  ["stable"],
);
const browser = await chromium.launch({ headless: true });
try {
  for (const phase of ["clear-all", "source", "delete"]) {
    if (selected !== "all" && selected !== phase) continue;
    await withMigrationPage(browser, server, async (page) => {
      const metrics = [];
      page.on("console", (event) => {
        if (event.text().startsWith("MIGRATION_HANDLES "))
          metrics.push(
            JSON.parse(event.text().slice("MIGRATION_HANDLES ".length)),
          );
      });
      const observation = page.waitForEvent("console", {
        predicate: (event) => event.text().startsWith("MIGRATION_HANDLES "),
        timeout: 20000,
      });
      const result = await page.evaluate(async (phase) => {
        const { fixtureCall } = await import("./support.mjs");
        const bytes = new Uint8Array(
          await (await fetch("/row-fixture/stable")).arrayBuffer(),
        );
        await fixtureCall({ operation: "import", path: "/source.db3", bytes });
        const sdk = await import("/target/sdk-packages/browser/entry.js");
        const OriginalWorker = globalThis.Worker;
        globalThis.Worker = class extends OriginalWorker {
          constructor(url, options) {
            super(
              new URL(url, location.href).pathname.endsWith(
                "/worker-entry.gen.js",
              )
                ? new URL(
                    "./lifecycle-worker.mjs?phase=" + phase,
                    location.href,
                  )
                : url,
              options,
            );
          }
        };
        let failure;
        try {
          await sdk.prepareMigrationArchive({
            databasePath: phase === "source" ? "/missing.db3" : "/source.db3",
            archiveKey: new Uint8Array(32).fill(7),
            outputPath: "failure.xmtp",
          });
        } catch (error) {
          failure =
            phase === "source"
              ? error instanceof sdk.XmtpError.InvalidInput
              : error instanceof sdk.XmtpError.MigrationOutput;
        } finally {
          globalThis.Worker = OriginalWorker;
        }
        return failure;
      }, phase);
      await observation;
      assert.equal(result, true, phase + ": wrong error category");
      assert.equal(
        metrics.length,
        1,
        phase + ": missing pre-termination observation",
      );
      const observed = metrics[0];
      console.log(JSON.stringify(observed));
      assert.ok(observed.opened.source > 0, "source handles were not observed");
      if (phase !== "source") {
        assert.ok(observed.faultCount > 0, "private fault did not fire");
        assert.ok(
          observed.opened.working > 0,
          "private handles were not observed",
        );
        if (phase === "clear-all") assert.equal(observed.privateAttempts, 8);
        if (phase === "delete")
          assert.ok(observed.deleteFaults > 0, "deletion fault did not fire");
        assert.equal(
          observed.closed.working,
          observed.opened.working,
          "private handles remain before worker termination",
        );
      }
      assert.equal(
        observed.closed.source,
        observed.opened.source,
        "source handles remain before worker termination",
      );
      const retry = await page.evaluate(async () => {
        const sdk = await import("/target/sdk-packages/browser/entry.js");
        return String(
          (
            await sdk.prepareMigrationArchive({
              databasePath: "/source.db3",
              archiveKey: new Uint8Array(32).fill(7),
              outputPath: "retry.xmtp",
            })
          ).messageCount,
        );
      });
      assert.equal(retry, "3", "retry failed after source handoff");
      console.log(
        phase + ": cleanup before worker termination and retry passed",
      );
    });
  }
  if (selected === "all" || selected === "fixture") {
    await withMigrationPage(browser, server, async (page) => {
      await page.evaluate(async () => {
        const { fixtureCall, check } = await import("./support.mjs");
        const bytes = new Uint8Array(
          await (await fetch("/row-fixture/stable")).arrayBuffer(),
        );
        const OriginalWorker = globalThis.Worker;
        let release;
        let requested;
        const termination = new Promise((resolve) => {
          requested = resolve;
        });
        globalThis.Worker = class extends OriginalWorker {
          terminate() {
            release = () => super.terminate();
            requested();
          }
        };
        let settled = false;
        const operation = fixtureCall({
          operation: "import",
          path: "/delayed.db3",
          bytes,
          lifetimeLock: "delayed-fixture:" + crypto.randomUUID(),
        }).finally(() => {
          settled = true;
        });
        await termination;
        // Hold the real worker well beyond the old 25 ms grace period.
        await new Promise((resolve) => setTimeout(resolve, 100));
        try {
          check(
            !settled,
            "fixtureCall settled before the worker released its lifetime lock",
          );
        } finally {
          release();
          globalThis.Worker = OriginalWorker;
          await operation;
        }
        const sdk = await import("/target/sdk-packages/browser/entry.js");
        const report = await sdk.prepareMigrationArchive({
          databasePath: "/delayed.db3",
          archiveKey: new Uint8Array(32).fill(7),
          outputPath: "delayed.xmtp",
        });
        check(
          report.messageCount === 3n,
          "migration failed after delayed fixture termination",
        );
      });
      console.log(
        "fixture: held worker blocks helper settlement and next migration succeeds",
      );
    });
  }
} finally {
  await browser.close();
  await server.close();
}
