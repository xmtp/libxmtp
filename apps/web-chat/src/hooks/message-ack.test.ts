import { act, renderHook } from "@testing-library/react";
import { MessageStream, type Message } from "@xmtp/browser-sdk";
import { afterEach, expect, it, vi } from "vitest";

import { inboxStore } from "@/stores/inbox/store";

import { useConversations } from "./useConversations";

const { client } = vi.hoisted(() => ({
  client: {
    conversations: {
      getMessageById: vi.fn(),
    },
  },
}));

vi.mock("@/contexts/XMTPContext", () => ({ useClient: () => client }));

const message = {
  id: "message",
  conversationId: "conversation",
  content: { kind: "text", value: "hello" },
} as Message;

const reaction = {
  id: "reaction",
  conversationId: "conversation",
  content: { kind: "reaction", reference: "message" },
} as Message;

const deferred = <T>() => {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
};

const callback = async () => {
  let deliver: ((value: Message) => void | Promise<void>) | undefined;
  vi.spyOn(MessageStream, "open").mockReturnValue({
    ready: async () => {},
    onValue: async (handler: (value: Message) => void | Promise<void>) => {
      deliver = handler;
    },
    end: async () => {},
  } as unknown as MessageStream);
  const { result, unmount } = renderHook(useConversations);
  await act(async () => {
    await result.current.streamAllMessages();
  });
  if (!deliver) throw new Error("message callback was not registered");
  return { deliver, unmount };
};

afterEach(() => {
  inboxStore.getState().reset();
  vi.restoreAllMocks();
  vi.clearAllMocks();
});

it("holds message acknowledgement until the store accepts the message", async () => {
  const write = deferred<void>();
  const addMessage = vi.fn(() => write.promise);
  inboxStore.setState({ addMessage });
  const { deliver, unmount } = await callback();

  let acknowledged = false;
  const delivery = Promise.resolve(deliver(message)).then(() => {
    acknowledged = true;
  });
  await Promise.resolve();
  expect(acknowledged).toBe(false);
  expect(addMessage).toHaveBeenCalledWith("conversation", message);

  write.resolve();
  await delivery;
  expect(acknowledged).toBe(true);
  unmount();
});

it("waits for reaction lookup and rejects a failed store update", async () => {
  const lookup = deferred<Message | undefined>();
  client.conversations.getMessageById.mockReturnValue(lookup.promise);
  const failure = new Error("store update failed");
  const addMessage = vi.fn(async () => {
    throw failure;
  });
  inboxStore.setState({ addMessage });
  const { deliver, unmount } = await callback();

  const delivery = Promise.resolve(deliver(reaction));
  expect(client.conversations.getMessageById).toHaveBeenCalledWith("message");
  expect(addMessage).not.toHaveBeenCalled();
  lookup.resolve(message);
  await expect(delivery).rejects.toBe(failure);
  expect(addMessage).toHaveBeenCalledWith("conversation", message);
  unmount();
});
