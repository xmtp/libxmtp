import { Client, type Identifier, type Signer } from "@xmtp/browser-sdk";

export async function createClient(signer: Signer) {
  // #region create
  const client = await Client.create(signer, {
    backendUrl: "https://xmtp.example.com",
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

export function closeClient(client: Client) {
  // #region delete
  // The Browser SDK cannot delete the local database.
  // This call only terminates the associated web worker.
  client.close();
  // #endregion delete
}
