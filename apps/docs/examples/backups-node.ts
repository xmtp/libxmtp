import type { Client } from "@xmtp/node-sdk";

export async function createBackup(client: Client, key: Uint8Array) {
  // #region create
  await client.archives.exportToFile("/path/to/archive.xmtp", key, {
    elements: ["messages", "consent"],
    excludeDisappearingMessages: true,
  });
  // #endregion create
}

export async function importBackup(client: Client, key: Uint8Array) {
  // #region import
  await client.archives.importFromFile("/path/to/archive.xmtp", key);
  // #endregion import
}

export async function readBackupMetadata(client: Client, key: Uint8Array) {
  // #region metadata
  const metadata = await client.archives.metadataFromFile(
    "/path/to/archive.xmtp",
    key,
  );
  // #endregion metadata
  return metadata;
}
