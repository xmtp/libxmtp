import { Client, type Identifier, type Signer } from "@xmtp/node-sdk";

export async function createClient(
  signer: Signer,
  dbEncryptionKey: Uint8Array,
) {
  // #region create
  const client = await Client.create(signer, {
    backendUrl: "https://xmtp.example.com",
    dbEncryptionKey,
  });
  // #endregion create
  return client;
}

export async function buildClient(
  identifier: Identifier,
  options: Parameters<typeof Client.build>[1],
) {
  // #region build
  const client = await Client.build(identifier, options);
  // #endregion build
  return client;
}

export async function deleteClient(client: Client) {
  // #region delete
  // The Node SDK has no database deletion method.
  // Close the client, then delete its database file from the file system.
  // #endregion delete
  await client.close();
}
