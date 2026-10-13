import { act, renderHook, waitFor } from "@testing-library/react";
import type { Client, ClientEvent, Conversation } from "@xmtp/browser-sdk";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { inboxStore } from "@/stores/inbox/store";

import { useIdentity } from "./useIdentity";
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
  // deliver an event without waiting for its retries to finish
  const fire = async (event: ClientEvent) => {
    if (!listener) throw new Error("no handle.changed listener");
    await act(async () => {
      void listener?.(event);
      await Promise.resolve();
    });
  };
  return { client, emit, fire };
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
      await result.current.refresh(true);
    });
    expect(result.current).toMatchObject({
      loaded: true,
      failed: false,
      handle: "alice",
    });
  });

  it("ignores a failed read for a client that the app switched away from", async () => {
    const a = fakeClient("a".repeat(64));
    const b = fakeClient("b".repeat(64));
    const slowA = Promise.withResolvers<Handles>();
    const slowB = Promise.withResolvers<Handles>();
    a.client.inboxState.mockReturnValueOnce(slowA.promise);
    b.client.inboxState.mockReturnValueOnce(slowB.promise);
    vi.spyOn(console, "error").mockImplementation(() => undefined);

    const { rerender } = renderSync(a.client);
    await waitFor(() => expect(a.client.inboxState).toHaveBeenCalled());
    rerender({ current: b.client });
    await waitFor(() => expect(b.client.inboxState).toHaveBeenCalled());

    await act(async () => slowA.reject(new Error("offline")));
    expect(inboxStore.getState().ownHandlesFailed).toBe(false);
    await act(async () => slowB.resolve({ handle: "bob", otherHandles: [] }));
    expect(inboxStore.getState().ownHandles?.handle).toBe("bob");
  });

  it("drops a slow earlier read that finishes after a newer refresh", async () => {
    const a = fakeClient("a".repeat(64));
    const slow = Promise.withResolvers<Handles>();
    a.client.inboxState
      .mockReturnValueOnce(slow.promise)
      .mockResolvedValueOnce({ handle: "alice2", otherHandles: ["alice"] });
    renderSync(a.client);
    await waitFor(() => expect(a.client.inboxState).toHaveBeenCalledOnce());

    mocks.client = a.client;
    const { result } = renderHook(() => useOwnHandles());
    await act(async () => {
      await result.current.refresh(true);
    });
    expect(inboxStore.getState().ownHandles?.handle).toBe("alice2");

    await act(async () => slow.resolve({ handle: "alice", otherHandles: [] }));
    expect(inboxStore.getState().ownHandles).toEqual({
      handle: "alice2",
      otherHandles: ["alice"],
    });
  });

  it("lets an earlier read fill the store after a newer read failed", async () => {
    const a = fakeClient("a".repeat(64));
    const slow = Promise.withResolvers<Handles>();
    a.client.inboxState
      .mockReturnValueOnce(slow.promise)
      .mockRejectedValueOnce(new Error("offline"));
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    renderSync(a.client);
    await waitFor(() => expect(a.client.inboxState).toHaveBeenCalledOnce());

    mocks.client = a.client;
    const { result } = renderHook(() => useOwnHandles());
    await act(async () => {
      await result.current.refresh(true).catch(() => undefined);
    });
    expect(inboxStore.getState().ownHandlesFailed).toBe(true);

    await act(async () => slow.resolve({ handle: "alice", otherHandles: [] }));
    expect(inboxStore.getState().ownHandles?.handle).toBe("alice");
    expect(result.current).toMatchObject({ loaded: true, failed: false });
  });

  it("drops a slow identity read that finishes after a newer refresh", async () => {
    const a = fakeClient("a".repeat(64));
    const identityRead = Promise.withResolvers<Handles>();
    a.client.inboxState
      .mockResolvedValueOnce({ handle: "alice", otherHandles: [] })
      .mockReturnValueOnce(identityRead.promise)
      .mockResolvedValueOnce({ handle: "alice2", otherHandles: ["alice"] });
    const client = Object.assign(a.client, {
      keyPackageStatuses: vi.fn(async () => new Map()),
    });
    renderSync(client);
    await waitFor(() =>
      expect(inboxStore.getState().ownHandles?.handle).toBe("alice"),
    );

    mocks.client = client;
    // the identity modal starts its read, then a newer refresh stores alice2
    renderHook(() => useIdentity(true));
    await waitFor(() => expect(a.client.inboxState).toHaveBeenCalledTimes(2));
    const { result } = renderHook(() => useOwnHandles());
    await act(async () => {
      await result.current.refresh(true);
    });
    expect(inboxStore.getState().ownHandles?.handle).toBe("alice2");

    await act(async () =>
      identityRead.resolve({
        inboxId: a.client.inboxId,
        installations: [],
        identities: [],
        recoveryIdentity: null,
        handle: "alice",
        otherHandles: [],
      } as unknown as Handles),
    );
    expect(inboxStore.getState().ownHandles).toEqual({
      handle: "alice2",
      otherHandles: ["alice"],
    });
  });

  describe("after handle.changed", () => {
    beforeEach(() => {
      vi.useFakeTimers();
      vi.spyOn(console, "error").mockImplementation(() => undefined);
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    // settle the first load and the listener registration
    const flush = () => act(async () => {});

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
      renderSync(a.client);
      await flush();
      expect(inboxStore.getState().ownHandles?.handle).toBe("alice");

      await a.fire(handleChanged(a.client.inboxId, "alice2"));
      expect(inboxStore.getState().ownHandles).toEqual({
        handle: "alice2",
        otherHandles: ["alice"],
      });
    });

    it("retries a failed own-handle refresh after a short delay", async () => {
      const a = fakeClient("a".repeat(64));
      a.client.inboxState
        .mockResolvedValueOnce({ handle: "alice", otherHandles: [] })
        .mockRejectedValueOnce(new Error("offline"))
        .mockResolvedValueOnce({ handle: "alice2", otherHandles: ["alice"] });
      renderSync(a.client);
      await flush();

      await a.fire(handleChanged(a.client.inboxId, "alice2"));
      expect(inboxStore.getState().ownHandles?.handle).toBe("alice");
      await act(async () => {
        await vi.advanceTimersByTimeAsync(1_000);
      });
      expect(a.client.inboxState).toHaveBeenCalledTimes(3);
      expect(inboxStore.getState().ownHandles?.handle).toBe("alice2");
    });

    it("stops retrying after the app switches clients", async () => {
      const a = fakeClient("a".repeat(64));
      const b = fakeClient("b".repeat(64));
      a.client.inboxState
        .mockResolvedValueOnce({ handle: "alice", otherHandles: [] })
        .mockRejectedValueOnce(new Error("offline"))
        .mockResolvedValue({ handle: "alice3", otherHandles: [] });
      b.client.inboxState.mockResolvedValue({
        handle: "bob",
        otherHandles: [],
      });
      const { rerender } = renderSync(a.client);
      await flush();
      await a.fire(handleChanged(a.client.inboxId, "alice2"));
      expect(a.client.inboxState).toHaveBeenCalledTimes(2);

      rerender({ current: b.client });
      await flush();
      await act(async () => {
        await vi.advanceTimersByTimeAsync(10_000);
      });
      expect(a.client.inboxState).toHaveBeenCalledTimes(2);
      expect(inboxStore.getState().ownHandles?.handle).toBe("bob");
    });
  });
});
