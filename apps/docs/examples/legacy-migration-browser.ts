// #region imports
import {
  prepareMigrationArchive,
  readMigrationArchive,
} from "@xmtp/browser-migration";
import type { Client } from "@xmtp/browser-sdk";
// #endregion imports

export async function migrateLegacyBrowserHistory(
  openDestination: () => Promise<Client>,
  databasePath: string,
  archiveKey: Uint8Array,
  outputPath: string,
) {
  // #region migrate
  const archive = await prepareMigrationArchive({
    databasePath,
    archiveKey,
    outputPath,
  });
  const data = await readMigrationArchive(archive.archivePath);
  const client = await openDestination();
  await client.archives.importFromBytes(data, archiveKey);
  // #endregion migrate
  return archive;
}
