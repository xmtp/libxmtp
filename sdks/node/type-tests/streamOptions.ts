import type { Client } from "@xmtp/node-sdk";

export async function checkStreamOptions(client: Client): Promise<void> {
  // @ts-expect-error Node notification streams have no pre-sync switch.
  await client.conversations.stream({ disableSync: true });
  // @ts-expect-error Preferences use the same public option shape.
  await client.preferences.streamPreferences({ disableSync: true });
  // @ts-expect-error Durable message readers also reject the old option.
  await client.conversations.streamAllMessages({ disableSync: true });
}
