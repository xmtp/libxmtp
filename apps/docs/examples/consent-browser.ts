import type { Client, Consent } from "@xmtp/browser-sdk";

export async function streamConsent(
  client: Client,
  handleConsent: (consent: Consent[]) => void,
) {
  // #region stream
  const stream = await client.preferences.streamConsent({
    onValue: handleConsent,
  });
  // #endregion stream
  return stream;
}
