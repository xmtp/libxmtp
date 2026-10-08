import { execFileSync } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { chromium } from "../../../node_modules/playwright/index.mjs";
import { createMigrationServer, withMigrationPage } from "./server.mjs";

const limit = 64 * 1024 * 1024;
const temporary = await mkdtemp(join(tmpdir(), "migration-row-size-"));
const cases = [
  "group-added",
  "group-dm",
  "group-paused",
  "group-combined",
  "group-unicode",
  "consent-conversation",
  "consent-inbox",
  "consent-unicode",
  "consent-nul",
  "group-boundary",
  "consent-boundary-conversation",
  "consent-boundary-inbox",
];
execFileSync("python3", [
  "-c",
  `
import shutil, sqlite3, sys
from pathlib import Path
limit=64*1024*1024
group_id=bytes([0x44])*16
cases = {
    'group-added': "added_by_inbox_id=printf('%%.*c', %d, 'a')" % (limit+1),
    'group-dm': "dm_id=printf('%%.*c', %d, 'd')" % (limit+1),
    'group-paused': "paused_for_version=CAST(zeroblob(%d) AS TEXT)" % (limit+1),
    'group-combined': "added_by_inbox_id=printf('%%.*c', %d, 'a'), dm_id=printf('%%.*c', %d, 'd')" % (limit//2,limit//2),
    'group-unicode': "added_by_inbox_id='éé', dm_id='d', paused_for_version=printf('%%.*c', %d, 'p')" % (limit-20),
    'group-boundary': "added_by_inbox_id='é', dm_id='d', paused_for_version=printf('%%.*c', %d, 'p')" % (limit-19),
    'consent-conversation': "entity_type=1, entity=printf('%%.*c', %d, 'c')" % (limit+1),
    'consent-inbox': "entity_type=2, entity=printf('%%.*c', %d, 'c')" % (limit+1),
    'consent-unicode': "entity_type=2, entity=printf('%%.*c', %d, 'c') || 'é'" % (limit-1),
    'consent-nul': "entity_type=1, entity=CAST(zeroblob(%d) AS TEXT)" % (limit+1),
    'consent-boundary-conversation': "entity_type=1, entity=printf('%%.*c', %d, 'c')" % limit,
    'consent-boundary-inbox': "entity_type=2, entity=printf('%%.*c', %d, 'c')" % limit,
}
for name, fields in cases.items():
    path=Path(sys.argv[1])/(name+'.db3')
    shutil.copyfile('crates/xmtp_legacy_migration/fixtures/stable.db3',path)
    connection=sqlite3.connect(path)
    if name.startswith('group-'):
        connection.execute("UPDATE groups SET added_by_inbox_id='', dm_id=NULL, paused_for_version=NULL WHERE id=?",(group_id,))
        connection.execute("UPDATE groups SET "+fields+" WHERE id=?",(group_id,))
    else:
        connection.execute("UPDATE consent_records SET "+fields)
    connection.commit()
    connection.close()
`,
  temporary,
]);
const server = await createMigrationServer(temporary, cases);
const browser = await chromium.launch({ headless: true });
try {
  for (const name of cases) {
    await withMigrationPage(browser, server, async (page) => {
      await page.evaluate(
        async ({ name, limit }) => {
          const sdk = await import("/target/sdk-packages/browser/entry.js");
          const {
            check,
            fixtureCall,
            exportSourceBytes,
            assertSourceUnchanged,
            assertFailedPreparation,
          } = await import("/sdks/browser/test/platform/migration/support.mjs");
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
          const before = await exportSourceBytes(source);
          const key = new Uint8Array(32).fill(7);
          const args = {
            databasePath: source,
            archiveKey: key,
            outputPath: "row-size.xmtp",
          };
          if (!name.includes("boundary")) {
            await assertFailedPreparation(sdk, args, name);
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
              await fixtureCall({
                operation: "record-sizes",
                bytes: archive,
                key,
              }),
            );
            check(
              records.length === 6 &&
                records.filter((record) => record.kind === "group").length ===
                  2 &&
                records.filter((record) => record.kind === "message").length ===
                  3 &&
                records.filter((record) => record.kind === "consent").length ===
                  1,
              "boundary archive record counts changed",
            );
            if (name === "group-boundary") {
              const group = records.find(
                (record) =>
                  record.kind === "group" && record.id === "44".repeat(16),
              );
              check(
                group.addedByBytes === 2 &&
                  group.addedByIsExpected &&
                  group.dmBytes === 1 &&
                  group.dmIsExpected &&
                  group.pauseBytes === limit - 19 &&
                  group.pauseIsExpected,
                "boundary group fields changed",
              );
            } else {
              const consent = records.find(
                (record) => record.kind === "consent",
              );
              check(
                consent.entityType ===
                  (name.endsWith("conversation") ? 1 : 2) &&
                  consent.entityBytes === limit &&
                  consent.entityIsExpected &&
                  consent.state === 2 &&
                  consent.consentedAtNs === "1700000000000000009",
                "boundary consent fields changed",
              );
            }
          }
          await assertSourceUnchanged(before, source);
        },
        { name, limit },
      );
      console.log(
        `${name}: 64 MiB row budget, source/output preservation, cleanup, and retry passed`,
      );
    });
  }
} finally {
  await browser.close();
  await server.close();
  await rm(temporary, { recursive: true, force: true });
}
