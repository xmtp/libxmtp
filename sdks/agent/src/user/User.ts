import type { PublicIdentity, Signer } from "@xmtp/node-sdk";
import {
  createWalletClient,
  http,
  toBytes,
  type Chain,
  type Hex,
  type PrivateKeyAccount,
  type WalletClient,
} from "viem";
import { generatePrivateKey, privateKeyToAccount } from "viem/accounts";
import { sepolia } from "viem/chains";

/** Local EOA material used to construct an Agent signer. */
export type User = {
  /** Private key used by the wallet. */
  key: Hex;
  /** viem account derived from {@link key}. */
  account: PrivateKeyAccount;
  /** viem wallet client used to sign messages. */
  wallet: WalletClient;
};

/** Create a viem wallet, generating a private key when one is not supplied. */
export const createUser = (key?: Hex, chain: Chain = sepolia): User => {
  const accountKey = key ?? generatePrivateKey();
  const account = privateKeyToAccount(accountKey);
  return {
    key: accountKey,
    account,
    wallet: createWalletClient({
      account,
      chain,
      transport: http(),
    }),
  };
};

/** Convert a user wallet address to the XMTP identifier shape. */
export const createIdentifier = (user: User): PublicIdentity => ({
  identifier: user.account.address.toLowerCase(),
  kind: "ethereum",
});

/** Adapt a viem wallet to the byte-returning XMTP signer interface. */
export const createSigner = (user: User): Signer => {
  const identifier = createIdentifier(user);
  return {
    identity: () => Promise.resolve(identifier),
    kind: () => Promise.resolve({ kind: "eoa" as const }),
    sign: async ({ text }) => ({
      kind: "ecdsa",
      value: toBytes(
        await user.wallet.signMessage({ account: user.account, message: text }),
      ),
    }),
  };
};
