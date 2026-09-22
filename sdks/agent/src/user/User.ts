import {
  IdentifierKind,
  type HexString,
  type Identifier,
  type Signer,
} from "@xmtp/node-sdk";
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
export const createUser = (key?: HexString, chain: Chain = sepolia): User => {
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
export const createIdentifier = (user: User): Identifier => ({
  identifier: user.account.address.toLowerCase(),
  identifierKind: IdentifierKind.Ethereum,
});

/** Adapt a viem wallet to the byte-returning XMTP signer interface. */
export const createSigner = (user: User): Signer => {
  const identifier = createIdentifier(user);
  return {
    type: "EOA",
    getIdentifier: () => identifier,
    signMessage: async (message: string) => {
      const signature = await user.wallet.signMessage({
        account: user.account,
        message,
      });
      return toBytes(signature);
    },
  };
};
