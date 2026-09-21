import { act, renderHook } from "@testing-library/react";
import type { Conversation } from "@xmtp/browser-sdk";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ContentTypes } from "@/contexts/XMTPContext";
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
    expect(client.conversations.list).toHaveBeenLastCalledWith({
      createdAfterNs: 42n,
    });
    unmount();
  });

  it("keeps message sync stable after message progress changes", async () => {
    const conversation = {
      id: "conversation",
      isActive: vi.fn(async () => true),
      sync: vi.fn(async () => {}),
      messages: vi.fn(async () => []),
    };
    inboxStore.setState({
      conversations: new Map([
        [
          conversation.id,
          conversation as unknown as Conversation<ContentTypes>,
        ],
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
      sentAfterNs: 84n,
    });
    unmount();
  });
});
