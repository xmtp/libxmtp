import { IdentifierKind, type Signer } from "@xmtp/node-sdk";

export function createSigner(
  address: string,
  signatureBytes: Uint8Array,
): Signer {
  // #region signer
  const signer = {
    type: "EOA" as const,
    getIdentifier: () => ({
      identifier: address,
      identifierKind: IdentifierKind.Ethereum,
    }),
    signMessage: async (message: string) => signatureBytes,
  };
  // #endregion signer
  return signer;
}
