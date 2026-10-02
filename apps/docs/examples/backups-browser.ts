import type { Client } from "@xmtp/browser-sdk";

export async function createBackup(client: Client, key: Uint8Array) {
  // #region create
  const archiveData = await client.archives.exportToBytes(key, {
    elements: ["consent"],
    excludeDisappearingMessages: true,
  });
  // #endregion create
  return archiveData;
}

export async function importBackup(
  client: Client,
  data: Uint8Array,
  key: Uint8Array,
) {
  // #region import
  await client.archives.importFromBytes(data, key);
  // #endregion import
}
