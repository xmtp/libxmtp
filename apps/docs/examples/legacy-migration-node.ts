// #region imports
import { prepareMigrationArchive } from "@xmtp/node-sdk";
import type { Client } from "@xmtp/node-sdk";
// #endregion imports

export async function migrateLegacyHistory(
  client: Client,
  databasePath: string,
  databaseKey: Uint8Array | undefined,
  archiveKey: Uint8Array,
  outputPath: string,
) {
  // #region migrate
  const archive = await prepareMigrationArchive({
    databasePath,
    databaseKey,
    archiveKey,
    outputPath,
  });
  await client.archives.importFromFile(archive.archivePath, archiveKey);
  // #endregion migrate
  return archive;
}
