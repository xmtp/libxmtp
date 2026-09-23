import type { Client } from "@/Client";

export async function checkMessageStreamOptions(client: Client): Promise<void> {
  // @ts-expect-error No message-stream retry count.
  await client.conversations.streamAllMessages({ retryAttempts: 1 });
  // @ts-expect-error No message-stream retry delay.
  await client.conversations.streamAllMessages({ retryDelay: 1 });
  // @ts-expect-error No message-stream retry switch.
  await client.conversations.streamAllMessages({ retryOnFail: false });
  // @ts-expect-error No message-stream retry callback.
  await client.conversations.streamAllMessages({ onRetry: () => {} });
  // @ts-expect-error No message-stream sync switch.
  await client.conversations.streamAllMessages({ disableSync: true });

  await client.conversations.stream({
    retryAttempts: 1,
    retryDelay: 1,
    retryOnFail: false,
    onRetry: () => {},
    disableSync: true,
  });
}
