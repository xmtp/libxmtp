import { act, renderHook } from "@testing-library/react";
import {
  Timestamp,
  type Conversation,
  type ListMessagesOptions,
  type Message as XmtpMessage,
} from "@xmtp/browser-sdk";
import { afterEach, describe, expect, it, vi } from "vitest";

import { inboxStore } from "@/stores/inbox/store";

import { useConversation } from "./useConversation";
import { useConversations } from "./useConversations";

const { client } = vi.hoisted(() => ({
  client: {
    conversations: {
      list: vi.fn(async () => []),
      sync: vi.fn(async () => {}),
      syncAll: vi.fn(async () => {}),
    },
  },
}));

vi.mock("@/contexts/XMTPContext", () => ({ useClient: () => client }));

afterEach(() => {
  inboxStore.getState().reset();
  vi.clearAllMocks();
});

describe("sync callbacks", () => {
  it("keeps list and stream callbacks stable while reading the latest cursor", async () => {
    const { result, rerender, unmount } = renderHook(useConversations);
    const { sync, syncAll, stream, streamAllMessages } = result.current;

    await act(async () => {
      await sync(true);
    });
    act(() => {
      inboxStore.setState({ lastCreatedAt: 42n });
    });
    rerender();

    expect(result.current.sync).toBe(sync);
    expect(result.current.syncAll).toBe(syncAll);
    expect(result.current.stream).toBe(stream);
    expect(result.current.streamAllMessages).toBe(streamAllMessages);
    await act(async () => {
      await result.current.sync();
    });
    expect(client.conversations.list).toHaveBeenLastCalledWith(
      expect.objectContaining({ createdAfter: new Timestamp(42n) }),
    );
    unmount();
  });

  it("keeps message sync stable after message progress changes", async () => {
    const conversation = {
      id: "conversation",
      state: vi.fn(async () => ({ isActive: true })),
      sync: vi.fn(async () => {}),
      messages: vi.fn(async () => []),
    };
    inboxStore.setState({
      conversations: new Map([
        [conversation.id, conversation as unknown as Conversation],
      ]),
    });
    const { result, rerender, unmount } = renderHook(() =>
      useConversation(conversation.id),
    );
    const { sync } = result.current;

    await act(async () => {
      await sync(true);
    });
    act(() => {
      inboxStore.setState({ lastSentAt: new Map([[conversation.id, 84n]]) });
    });
    rerender();

    expect(result.current.sync).toBe(sync);
    await act(async () => {
      await result.current.sync();
    });
    expect(conversation.messages).toHaveBeenLastCalledWith({
      sentAfter: new Timestamp(84n),
      direction: "ascending",
      limit: 1000,
    });
    unmount();
  });

  it("reads every message after the cursor in pages", async () => {
    // Message 1200 shares a sent time with message 1199, the last one on the
    // first page after the cursor. Message 500 cannot be converted, so the
    // first page is short.
    const history = messageHistory(2500, (index) =>
      BigInt(index === 1200 ? 1200 : index + 1),
    );
    const stored = await syncAfter(
      pagedMessages(history, new Set(["m500"])),
      200n,
    );

    expect(stored.ids).toEqual(
      history
        .slice(200)
        .map((message) => message.id)
        .filter((id) => id !== "m500")
        .sort(),
    );
    expect(stored.lastSentAt).toBe(2500n);
  });

  it("finishes when more than a page of messages share one sent time", async () => {
    const messages = pagedMessages(messageHistory(1500, () => 5n));
    const stored = await syncAfter(messages, undefined);

    expect(stored.lastSentAt).toBe(5n);
    expect(messages).toHaveBeenCalledTimes(3);
  });
});

const messageHistory = (length: number, sentAt: (index: number) => bigint) =>
  Array.from({ length }, (_, index) => ({
    id: `m${index}`,
    conversationId: "conversation",
    sentAt: { ns: sentAt(index) },
    content: { kind: "text" },
  })) as unknown as XmtpMessage[];

/** The SDK page rules: the first `limit` (1 to 1000) in the requested order,
 * or the newest 100 when no limit is named. `dropped` rows leave a page after
 * the limit, as rows the SDK cannot convert do. */
const pagedMessages = (history: XmtpMessage[], dropped = new Set<string>()) => {
  let calls = 0;
  return vi.fn(async (options: ListMessagesOptions = {}) => {
    // A loop that never finishes fails the test instead of hanging it.
    calls += 1;
    if (calls > 20) throw new Error("too many pages");
    const { direction, limit, sentAfter } = options;
    if (limit !== undefined && (limit < 1 || limit > 1000)) {
      throw new Error("InvalidArgument");
    }
    const matching = history.filter(
      (message) => sentAfter === undefined || message.sentAt.ns > sentAfter.ns,
    );
    const ordered =
      direction === "descending" ? [...matching].reverse() : matching;
    let page = ordered.slice(0, limit);
    if (limit === undefined) {
      page = matching.slice(-100);
      if (direction === "descending") page.reverse();
    }
    return page.filter((message) => !dropped.has(message.id));
  });
};

const syncAfter = async (
  messages: ReturnType<typeof pagedMessages>,
  lastSentAt: bigint | undefined,
) => {
  const conversation = { id: "conversation", messages };
  inboxStore.setState({
    conversations: new Map([
      [conversation.id, conversation as unknown as Conversation],
    ]),
    lastSentAt: new Map([[conversation.id, lastSentAt]]),
  });
  const { result, unmount } = renderHook(() =>
    useConversation(conversation.id),
  );
  try {
    await act(async () => {
      await result.current.sync();
    });
  } finally {
    unmount();
  }
  const state = inboxStore.getState();
  return {
    ids: [...(state.messages.get(conversation.id)?.keys() ?? [])].sort(),
    lastSentAt: state.lastSentAt.get(conversation.id),
  };
};
