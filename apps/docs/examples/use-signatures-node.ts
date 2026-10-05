import type { Client } from "@xmtp/node-sdk";

export async function signText(client: Client, signatureText: string) {
  // #region sign
  const signature = await client.signWithInstallationKey(signatureText);
  // #endregion sign
  return signature;
}
