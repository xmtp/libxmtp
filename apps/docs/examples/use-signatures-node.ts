import { Client } from "@xmtp/node-sdk";

export async function signText(client: Client, signatureText: string) {
  // #region sign
  const signature = await client.signWithInstallationKey(signatureText);
  // #endregion sign
  return signature;
}

export async function verifyOwnSignature(
  client: Client,
  signatureText: string,
  signature: Uint8Array,
) {
  // #region verify
  const valid = await client.verifySignedWithInstallationKey(
    signatureText,
    signature,
  );
  // #endregion verify
  return valid;
}

export async function verifyOtherSignature(
  signatureText: string,
  signature: Uint8Array,
  installationPublicKey: Uint8Array,
) {
  // #region verify-other
  const valid = await Client.verifySignedWithPublicKey(
    signatureText,
    signature,
    installationPublicKey,
  );
  // #endregion verify-other
  return valid;
}
