import type { Client, ClientEvent } from "@xmtp/node-sdk";

type ConsentChange = Extract<ClientEvent, { kind: "consentChanged" }>;

export async function streamConsent(
  client: Client,
  handleConsent: (consent: ConsentChange) => void,
) {
  // #region stream
  const stream = await client.events({
    kinds: ["consentChanged"],
    referencesOwnMessages: false,
  });
  const receive = (async () => {
    for await (const event of stream) {
      if (event.kind === "consentChanged") handleConsent(event);
    }
  })();
  // #endregion stream
  return { stream, receive };
}
