import { act, renderHook, waitFor } from "@testing-library/react";
import type { Client, ClientEvent, Conversation } from "@xmtp/browser-sdk";
import { afterEach, describe, expect, it, vi } from "vitest";

import { inboxStore } from "@/stores/inbox/store";

import { useHandleSync } from "./useOwnHandles";

const member = (
  inboxId: string,
  handle?: string,
  otherHandles: string[] = [],
) => ({
  inboxId,
  identities: [],
  permissionLevel: "member" as const,
  consentState: "unknown" as const,
  handle,
  otherHandles,
});

const handleChanged = (inboxId: string, handle: string): ClientEvent => ({
  kind: "handle.changed",
  handle_changed: { inbox_id: inboxId, handle },
});

const setup = () => {
  const peer = "b".repeat(64);
  const own = "a".repeat(64);
  let listener: ((event: ClientEvent) => Promise<void> | void) | undefined;
  const conversation = {
    id: "group",
    members: vi.fn(async () => [
      member(own, "me"),
      member(peer, "nicky", ["nick#2341"]),
    ]),
  };
  const client = {
    inboxId: own,
    inboxState: vi.fn(async () => ({
      handle: "me",
      otherHandles: ["me#0001"],
    })),
    startListener: vi.fn(
      async (
        _filter: unknown,
        callback: (event: ClientEvent) => Promise<void> | void,
      ) => {
        listener = callback;
        return 7n;
      },
    ),
    stopListener: vi.fn(async () => {}),
  };
  inboxStore.setState({
    conversations: new Map([
      ["group", conversation as unknown as Conversation],
    ]),
    members: new Map([
      [
        "group",
        new Map([
          [own, member(own, "me")],
          [peer, member(peer, "nick#2341")],
        ]),
      ],
    ]),
  });
  const emit = async (event: ClientEvent) => {
    if (!listener) throw new Error("no handle.changed listener");
    await act(async () => listener?.(event));
  };
  return { client, conversation, emit, own, peer };
};

afterEach(() => {
  inboxStore.getState().reset();
});

describe("useHandleSync", () => {
  it("reads a member's new handle when handle.changed arrives", async () => {
    const { client, conversation, emit, peer } = setup();
    const { unmount } = renderHook(() =>
      useHandleSync(client as unknown as Client),
    );
    await waitFor(() => expect(client.startListener).toHaveBeenCalled());
    expect(client.startListener.mock.calls[0][0]).toEqual({
      kinds: ["handle.changed"],
    });
    await emit(handleChanged(peer, "nicky"));
    expect(conversation.members).toHaveBeenCalledTimes(1);
    const updated = inboxStore.getState().members.get("group")?.get(peer);
    expect(updated?.handle).toBe("nicky");
    expect(updated?.otherHandles).toEqual(["nick#2341"]);
    unmount();
    expect(client.stopListener).toHaveBeenCalledWith(7n);
  });

  it("reloads the client's own handles when its own handle changes", async () => {
    const { client, emit, own } = setup();
    renderHook(() => useHandleSync(client as unknown as Client));
    await waitFor(() => expect(client.startListener).toHaveBeenCalled());
    await waitFor(() =>
      expect(inboxStore.getState().ownHandles?.handle).toBe("me"),
    );
    client.inboxState.mockResolvedValueOnce({
      handle: "me2",
      otherHandles: ["me", "me#0001"],
    });
    await emit(handleChanged(own, "me2"));
    expect(inboxStore.getState().ownHandles).toEqual({
      handle: "me2",
      otherHandles: ["me", "me#0001"],
    });
  });
});
