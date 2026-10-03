import type { Signer } from "@xmtp/browser-sdk";
import { toBytes, type Hex } from "viem";
import { privateKeyToAccount } from "viem/accounts";

export const createEOASigner = (
  address: `0x${string}`,
  signMessage: (message: string) => Promise<string> | string,
): Signer => ({
  identity: () =>
    Promise.resolve({
      identifier: address.toLowerCase(),
      kind: "ethereum",
    }),
  kind: () => Promise.resolve({ kind: "eoa" }),
  sign: async (request) => ({
    kind: "ecdsa",
    value: toBytes(await signMessage(request.text)),
  }),
});

export const createEphemeralSigner = (privateKey: Hex): Signer => {
  const account = privateKeyToAccount(privateKey);
  return createEOASigner(account.address, (message) =>
    account.signMessage({ message }),
  );
};

export const createSCWSigner = (
  address: `0x${string}`,
  signMessage: (message: string) => Promise<string> | string,
  chainId: number = 1,
): Signer => ({
  identity: () =>
    Promise.resolve({
      identifier: address.toLowerCase(),
      kind: "ethereum",
    }),
  kind: () => Promise.resolve({ kind: "scw", chainId: BigInt(chainId) }),
  sign: async (request) => ({
    kind: "scw",
    bytes: toBytes(await signMessage(request.text)),
    address,
    chainId: BigInt(chainId),
  }),
});
