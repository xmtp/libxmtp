import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  ConsentEntityType,
  ConsentState,
  createClient as create,
  generateInboxId,
  getInboxIdForAddress,
  GroupMessageKind,
  LogLevel,
  SignatureRequestType,
  verifySignedWithPublicKey,
} from "legacy-bindings";
import { v4 } from "uuid";
import { createWalletClient, http, toBytes } from "viem";
import { generatePrivateKey, privateKeyToAccount } from "viem/accounts";
import { sepolia } from "viem/chains";

export const TEST_API_URL = "http://localhost:5556";

export const createOldUser = () => {
  const key = generatePrivateKey();
  const account = privateKeyToAccount(key);
  return {
    key,
    account,
    wallet: createWalletClient({
      account,
      chain: sepolia,
      transport: http(),
    }),
    uuid: v4(),
  };
};

export type User = ReturnType<typeof createOldUser>;

export const createOldClient = async (user: User) => {
  const dbPath = join(__dirname, `${user.uuid}.db3`);
  const inboxId =
    (await getInboxIdForAddress(TEST_API_URL, false, user.account.address)) ||
    generateInboxId(user.account.address);

  return create(
    TEST_API_URL,
    false,
    dbPath,
    inboxId,
    user.account.address,
    undefined,
    undefined,
    { level: LogLevel.off },
  );
};
export const createOldRegisteredClient = async (user: User) => {
  const client = await createOldClient(user);
  if (!client.isRegistered()) {
    const signatureText = await client.createInboxSignatureText();
    if (signatureText) {
      const signature = await user.wallet.signMessage({
        message: signatureText,
      });
      await client.addEcdsaSignature(
        SignatureRequestType.CreateInbox,
        toBytes(signature),
      );
    }
    await client.registerIdentity();
  }
  return client;
};
