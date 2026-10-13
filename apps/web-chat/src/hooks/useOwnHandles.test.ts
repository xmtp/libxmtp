import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import type { Client, ClientEvent, Conversation } from "@xmtp/browser-sdk";
import { afterEach, describe, expect, it, vi } from "vitest";

import { inboxStore } from "@/stores/inbox/store";

import { useHandleSync, useOwnHandles } from "./useOwnHandles";

const mocks = vi.hoisted(() => ({ client: undefined as unknown }));
vi.mock("@/contexts/XMTPContext", () => ({
  useClient: () => mocks.client,
  useXMTP: () => ({}),
}));

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
  // unmount first, so a store reset cannot restart an earlier test's sync
  cleanup();
  inboxStore.getState().reset();
  vi.restoreAllMocks();
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
  renderHook(() => useHandleSync(client as Client));

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

  it("reads the own handles only after the listener is registered", async () => {
    const a = fakeClient("a".repeat(64));
    const registered = Promise.withResolvers<bigint>();
    a.client.startListener.mockReturnValueOnce(registered.promise);
    a.client.inboxState.mockResolvedValue({
      handle: "alice",
      otherHandles: [],
    });
    renderSync(a.client);
    expect(a.client.startListener).toHaveBeenCalledOnce();
    // a handle.changed before the listener exists would be lost
    expect(a.client.inboxState).not.toHaveBeenCalled();

    await act(async () => registered.resolve(1n));
    await waitFor(() =>
      expect(inboxStore.getState().ownHandles?.handle).toBe("alice"),
    );
    expect(a.client.inboxState).toHaveBeenCalledExactlyOnceWith(false);
  });

  it("reads the own handles when the listener cannot start", async () => {
    const a = fakeClient("a".repeat(64));
    a.client.startListener.mockRejectedValueOnce(new Error("closed"));
    a.client.inboxState.mockResolvedValue({
      handle: "alice",
      otherHandles: [],
    });
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    renderSync(a.client);
    await waitFor(() =>
      expect(inboxStore.getState().ownHandles?.handle).toBe("alice"),
    );
  });

  it("marks a failed first read, and a working retry loads the handles", async () => {
    const a = fakeClient("a".repeat(64));
    a.client.inboxState
      .mockRejectedValueOnce(new Error("offline"))
      .mockResolvedValueOnce({ handle: "alice", otherHandles: [] });
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    renderSync(a.client);
    await waitFor(() =>
      expect(inboxStore.getState().ownHandlesFailed).toBe(true),
    );
    expect(inboxStore.getState().ownHandles).toBeUndefined();

    mocks.client = a.client;
    const { result } = renderHook(() => useOwnHandles());
    expect(result.current).toMatchObject({ loaded: false, failed: true });
    await act(async () => {
      await result.current.refresh();
    });
    expect(result.current).toMatchObject({
      loaded: true,
      failed: false,
      handle: "alice",
    });
    // the retry reads the local state again
    expect(a.client.inboxState).toHaveBeenLastCalledWith(false);
  });

  it("keeps the known handles when a later read fails", async () => {
    const a = fakeClient("a".repeat(64));
    a.client.inboxState
      .mockResolvedValueOnce({ handle: "alice", otherHandles: [] })
      .mockRejectedValueOnce(new Error("offline"));
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    renderSync(a.client);
    await waitFor(() =>
      expect(inboxStore.getState().ownHandles?.handle).toBe("alice"),
    );

    await a.emit(handleChanged(a.client.inboxId, "alice2"));
    expect(a.client.inboxState).toHaveBeenCalledTimes(2);
    mocks.client = a.client;
    const { result } = renderHook(() => useOwnHandles());
    expect(result.current).toMatchObject({
      loaded: true,
      failed: false,
      handle: "alice",
    });
  });

  it("reloads the own handles when the member refresh fails", async () => {
    const a = fakeClient("a".repeat(64));
    a.client.inboxState
      .mockResolvedValueOnce({ handle: "alice", otherHandles: [] })
      .mockResolvedValueOnce({ handle: "alice2", otherHandles: ["alice"] });
    inboxStore.setState({
      refreshInboxHandles: vi.fn(async () => {
        throw new Error("members read failed");
      }),
    });
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    renderSync(a.client);
    await waitFor(() =>
      expect(inboxStore.getState().ownHandles?.handle).toBe("alice"),
    );

    await a.emit(handleChanged(a.client.inboxId, "alice2"));
    expect(inboxStore.getState().ownHandles).toEqual({
      handle: "alice2",
      otherHandles: ["alice"],
    });
  });
});
