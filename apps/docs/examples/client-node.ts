import { Client, type PublicIdentity, type Signer } from "@xmtp/node-sdk";

export async function createClient(
  signer: Signer,
  dbEncryptionKey: Uint8Array,
) {
  // #region create
  const client = await Client.create(signer, {
    backend: { url: "https://xmtp.example.com" },
    storage: { location: "default", encryptionKey: dbEncryptionKey },
  });
  // #endregion create
  return client;
}

export async function buildClient(
  identity: PublicIdentity,
  options: Parameters<typeof Client.build>[1],
) {
  // #region build
  const client = await Client.build(identity, options);
  // #endregion build
  return client;
}

export async function createAuthenticatedClient(
  signer: Signer,
  fetchToken: () => Promise<{ token: string; expiresAtSeconds: bigint }>,
) {
  // #region auth
  const client = await Client.create(signer, {
    backend: {
      url: "https://xmtp.example.com",
      credentials: {
        async credential() {
          const { token, expiresAtSeconds } = await fetchToken();
          return { value: `Bearer ${token}`, expiresAtSeconds };
        },
      },
    },
    storage: { location: "default" },
  });
  // #endregion auth
  return client;
}

export async function deleteClient(client: Client) {
  // #region delete
  // End the client and delete its persistent database.
  await client.storage.delete_();
  // #endregion delete
}
