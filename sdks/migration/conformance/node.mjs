import assert from "node:assert/strict";
import { copyFile, mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
const pkg = resolve(
  process.env.XMTP_MIGRATION_NODE_PACKAGE ?? "target/migration-packages/node",
);
const { prepareMigrationArchive, MigrationError } = await import(
  pathToFileURL(join(pkg, "index.js"))
);
const folder = await mkdtemp(join(tmpdir(), "xmtp-migration-node-"));
try {
  const source = join(folder, "legacy.db3");
  const fixture = resolve(
    "crates/xmtp_legacy_migration/fixtures/encrypted.db3",
  );
  const before = new Map();
  for (const suffix of ["", "-wal", ".sqlcipher_salt"]) {
    await copyFile(fixture + suffix, source + suffix);
    before.set(suffix, await readFile(source + suffix));
  }
  const keyBuffer = Buffer.allocUnsafe(96);
  keyBuffer.fill(0x55);
  const archiveKey = keyBuffer.subarray(13, 45);
  archiveKey.fill(7);
  const databaseKey = keyBuffer.subarray(49, 81);
  databaseKey.fill(0x11);
  const args = {
    databasePath: source,
    databaseKey,
    archiveKey,
    outputPath: join(folder, "history.xmtp"),
  };
  const report = await prepareMigrationArchive(args);
  assert.deepEqual(report, {
    archivePath: args.outputPath,
    groupCount: 2n,
    messageCount: 4n,
    consentCount: 1n,
  });
  const completed = await readFile(report.archivePath);
  assert.ok(completed.length > 32);
  await assert.rejects(
    prepareMigrationArchive({ ...args, databaseKey: new Uint8Array(32) }),
    MigrationError.InvalidInput.instanceOf,
  );
  await assert.rejects(
    prepareMigrationArchive({ ...args, archiveKey: new Uint8Array(31) }),
    MigrationError.InvalidInput.instanceOf,
  );
  assert.deepEqual(await readFile(report.archivePath), completed);
  for (const [suffix, bytes] of before)
    assert.deepEqual(await readFile(source + suffix), bytes);
  console.log(
    "Node package: encrypted WAL export, exact key byte views, bigint counts, typed errors, and source/output preservation passed",
  );
} finally {
  await rm(folder, { recursive: true, force: true });
}
