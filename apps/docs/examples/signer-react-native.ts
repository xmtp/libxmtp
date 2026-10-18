import { Client, LocalSigningKey, type Signer } from "@xmtp/react-native-sdk";

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
  const signer = await Client.generateKeys();
  // #endregion local
  return signer;
}

export async function restoreLocalSigner(saved: string) {
  // #region restore
  const signer = await LocalSigningKey.fromHex(saved);
  const keyBytes = await signer.toBytes();
  const keyHex = await signer.toHex();
  // #endregion restore
  return { signer, keyBytes, keyHex };
}
