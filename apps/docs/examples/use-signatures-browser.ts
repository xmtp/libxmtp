import type { Client } from "@xmtp/browser-sdk";

export function signText(client: Client, signatureText: string) {
  // #region sign
  const signature = client.signWithInstallationKey(signatureText);
  // #endregion sign
  return signature;
}
