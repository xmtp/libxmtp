import { Client, type ClientOptions, type Signer } from "@xmtp/browser-sdk";
import { toBytes } from "viem";
import { generatePrivateKey, privateKeyToAccount } from "viem/accounts";
import { afterEach } from "vitest";

const clients = new Set<Client>();
export const backend = { url: import.meta.env.XMTP_BACKEND_URL };
export const options: ClientOptions = {
  backend,
  storage: { location: "inMemory" },
  deviceSync: false,
};
export const signer = (): Signer => {
  const account = privateKeyToAccount(generatePrivateKey());
  return {
    async identity() {
      return { identifier: account.address.toLowerCase(), kind: "ethereum" };
    },
    async kind() {
      return { kind: "eoa" };
    },
    async sign(request) {
      return {
        kind: "ecdsa",
        value: toBytes(await account.signMessage({ message: request.text })),
      };
    },
  };
};
export const create = async (
  owner = signer(),
  settings: Partial<ClientOptions> = {},
) => {
  const client = await Client.create(owner, { ...options, ...settings });
  clients.add(client);
  return client;
};
afterEach(async () => {
  const results = await Promise.allSettled(
    [...clients].map((client) => client.end()),
  );
  clients.clear();
  for (const result of results)
    if (result.status === "rejected") throw result.reason;
});
