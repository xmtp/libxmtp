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

type Handles = { handle: string; otherHandles: string[] };

/** A client whose inboxState answers come from the caller. */
const fakeClient = (inboxId: string) => {
  let listener: ((event: ClientEvent) => Promise<void> | void) | undefined;
  const client = {
    inboxId,
    inboxState: vi.fn<() => Promise<Handles>>(),
    startListener: vi.fn(
      async (
        _filter: unknown,
        callback: (event: ClientEvent) => Promise<void> | void,
      ) => {
        listener = callback;
        return 1n;
      },
    ),
    stopListener: vi.fn(async () => {}),
  };
  const emit = async (event: ClientEvent) => {
    if (!listener) throw new Error("no handle.changed listener");
    await act(async () => listener?.(event));
  };
  return { client, emit };
};

const renderSync = (client: unknown) =>
  renderHook(({ current }) => useHandleSync(current as Client), {
    initialProps: { current: client },
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

  it("drops a slow read for a client that the app switched away from", async () => {
    const a = fakeClient("a".repeat(64));
    const b = fakeClient("b".repeat(64));
    const slow = Promise.withResolvers<Handles>();
    a.client.inboxState.mockReturnValueOnce(slow.promise);
    b.client.inboxState.mockResolvedValue({
      handle: "bob",
      otherHandles: [],
    });

    const { rerender } = renderSync(a.client);
    await waitFor(() => expect(a.client.inboxState).toHaveBeenCalled());
    rerender({ current: b.client });
    await waitFor(() =>
      expect(inboxStore.getState().ownHandles?.handle).toBe("bob"),
    );

    await act(async () => slow.resolve({ handle: "alice", otherHandles: [] }));
    expect(inboxStore.getState().ownHandles).toEqual({
      handle: "bob",
      otherHandles: [],
    });
  });

  it("drops a handle.changed read for a client that the app switched away from", async () => {
    const a = fakeClient("a".repeat(64));
    const b = fakeClient("b".repeat(64));
    a.client.inboxState.mockResolvedValue({
      handle: "alice",
      otherHandles: [],
    });
    b.client.inboxState.mockResolvedValue({ handle: "bob", otherHandles: [] });

    const { rerender } = renderSync(a.client);
    await waitFor(() => expect(a.client.startListener).toHaveBeenCalled());
    await waitFor(() =>
      expect(inboxStore.getState().ownHandles?.handle).toBe("alice"),
    );
    rerender({ current: b.client });
    await waitFor(() =>
      expect(inboxStore.getState().ownHandles?.handle).toBe("bob"),
    );

    // an event that A's listener delivers after the switch
    a.client.inboxState.mockResolvedValue({
      handle: "alice2",
      otherHandles: ["alice"],
    });
    await a.emit(handleChanged(a.client.inboxId, "alice2"));
    expect(a.client.inboxState).toHaveBeenCalledTimes(2);
    expect(inboxStore.getState().ownHandles).toEqual({
      handle: "bob",
      otherHandles: [],
    });
  });
});
