// #region app
import { Client, type Signer } from "@xmtp/react-native-sdk";

export async function connectMessaging(
  signer: Signer,
  backendUrl: string,
  databaseKey: Uint8Array,
  onText: (text: string) => Promise<void>,
  onError: (error: unknown) => void,
) {
  const client = await Client.create(signer, {
    backend: { url: backendUrl },
    storage: { location: "default", encryptionKey: databaseKey },
  });
  const messages = await client.conversations.streamAllMessages();
  const receiving = messages.onValue(async (message) => {
    if (message.content.kind === "text") {
      await onText(message.content.value);
    }
  });
  void receiving.catch(onError);
  return {
    inboxId: client.inboxId,
    async send(peerInboxId: string, text: string) {
      const dm = await client.conversations.createDm(peerInboxId);
      return dm.sendText(text);
    },
    async disconnect() {
      try {
        await messages.end();
        await receiving;
      } finally {
        await client.end();
      }
    },
  };
}
// #endregion app
