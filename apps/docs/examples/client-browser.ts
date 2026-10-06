import {
  Storage,
  Client,
  type PublicIdentity,
  type Signer,
} from "@xmtp/browser-sdk";

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

export async function createClient(signer: Signer) {
  // #region create
  const client = await Client.create(signer, {
    backend: { url: "https://xmtp.example.com" },
    storage: { location: "default" },
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

export async function deleteClient(client: Client) {
  // #region delete
  const path = await client.storage.path();
  await client.end();
  if (path !== undefined) {
    const admin = await Storage.admin();
    try {
      await admin.deleteFile(path);
    } finally {
      await admin.end();
    }
  }
  // #endregion delete
}
