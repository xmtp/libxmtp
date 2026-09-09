import { BackupElementSelectionOption, type Client } from "@xmtp/node-sdk";

export async function createBackup(client: Client, key: Uint8Array) {
  // #region create
  await client.createArchive("/path/to/archive.xmtp", key, {
    elements: [BackupElementSelectionOption.Consent],
    excludeDisappearingMessages: true,
  });
  // #endregion create
}

export async function importBackup(client: Client, key: Uint8Array) {
  // #region import
  await client.importArchive("/path/to/archive.xmtp", key);
  // #endregion import
}
