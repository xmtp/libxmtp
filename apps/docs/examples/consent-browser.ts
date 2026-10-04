import type { Client, ClientEvent } from "@xmtp/browser-sdk";

type ConsentChange = Extract<ClientEvent, { kind: "consent.changed" }>;

export async function streamConsent(
  client: Client,
  handleConsent: (consent: ConsentChange) => void,
) {
  // #region stream
  const stream = await client.events({
    kinds: ["consent.changed"],
    references_own_messages: false,
  });
  const receive = (async () => {
    for await (const event of stream) {
      if (event.kind === "consent.changed") handleConsent(event);
    }
  })();
  // #endregion stream
  return { stream, receive };
}
