import type { Client } from "@xmtp/node-sdk";

export function signText(client: Client, signatureText: string) {
  // #region sign
  const signature = client.signWithInstallationKey(signatureText);
  // #endregion sign
  return signature;
}
