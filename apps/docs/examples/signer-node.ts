import { generateLocalSigner, type Signer } from "@xmtp/node-sdk";

export function createSigner(
  address: string,
  signText: (text: string) => Promise<Uint8Array>,
): Signer {
  // #region signer
  const signer: Signer = {
    identity: async () => ({ identifier: address, kind: "ethereum" }),
    kind: async () => ({ kind: "eoa" }),
    sign: async (request) => ({
      kind: "ecdsa",
      value: await signText(request.text),
    }),
  };
  // #endregion signer
  return signer;
}

export async function createLocalSigner() {
  // #region local
  const signer = await generateLocalSigner();
  // #endregion local
  return signer;
}
