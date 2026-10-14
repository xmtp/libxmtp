import { execFileSync } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { chromium } from "../../../node_modules/playwright/index.mjs";
import { createMigrationServer, withMigrationPage } from "./server.mjs";

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
          if (name !== "boundary") {
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
