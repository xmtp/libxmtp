import type { Client, ClientEvent } from "@xmtp/node-sdk";
export async function streamConsent(
  client: Client,
  handleConsent: (
    event: Extract<ClientEvent, { kind: "consentChanged" }>,
  ) => void,
) {
  // #region stream
  const stream = await client.events({
    kinds: ["consentChanged"],
    referencesOwnMessages: false,
  });
  void (async () => {
    for await (const event of stream)
      if (event.kind === "consentChanged") handleConsent(event);
  })().catch(console.error);
  // #endregion stream
  return stream;
}
