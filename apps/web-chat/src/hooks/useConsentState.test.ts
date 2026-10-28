import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import type { Client, ClientEvent, Conversation } from "@xmtp/browser-sdk";
import { hexToBytes } from "viem";
import { afterEach, describe, expect, it, vi } from "vitest";

import { useConsentState } from "./useConsentState";

const setup = () => {
  let listener: ((event: ClientEvent) => Promise<void> | void) | undefined;
  const client = {
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
  const emit = async (event: ClientEvent) => {
    if (!listener) throw new Error("no consent.changed listener");
    await act(async () => listener?.(event));
  };
  return { client, emit };
};

const conversation = (id: string, state: () => Promise<unknown>) =>
  ({ id, state: vi.fn(state) }) as unknown as Conversation;

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("useConsentState", () => {
  it("reads consent again when it changes", async () => {
    const { client, emit } = setup();
    const id = "ab".repeat(16);
    let consent = "unknown";
    const current = conversation(id, async () => ({ consentState: consent }));
    const { result, unmount } = renderHook(() =>
      useConsentState(client as unknown as Client, current),
    );
    expect(result.current).toBeUndefined();
    await waitFor(() => {
      expect(result.current).toBe("unknown");
    });
    expect(client.startListener).toHaveBeenCalledWith(
      { kinds: ["consent.changed"], group_ids: [hexToBytes(`0x${id}`)] },
      expect.any(Function),
    );

    consent = "allowed";
    await emit({
      kind: "consent.changed",
      consent_changed: {
        entity_kind: "conversation",
        entity: id,
        state: "allowed",
      },
    });
    expect(result.current).toBe("allowed");

    unmount();
    expect(client.stopListener).toHaveBeenCalledWith(7n);
  });

  it("returns undefined, not the previous state, after switching", async () => {
    const { client } = setup();
    let firstState = async (): Promise<unknown> => ({
      consentState: "allowed",
    });
    const first = conversation("aa".repeat(16), () => firstState());
    const pending = Promise.withResolvers<unknown>();
    const second = conversation("bb".repeat(16), () => pending.promise);
    const { result, rerender } = renderHook(
      ({ current }) => useConsentState(client as unknown as Client, current),
      { initialProps: { current: first } },
    );
    await waitFor(() => {
      expect(result.current).toBe("allowed");
    });

    rerender({ current: second });
    expect(result.current).toBeUndefined();

    // the first conversation's consent may have changed while it was closed
    firstState = () => new Promise(() => {});
    rerender({ current: first });
    expect(result.current).toBeUndefined();
  });

  it("forgets an allowed state when a later read fails", async () => {
    const { client } = setup();
    const id = "cd".repeat(16);
    let state = async (): Promise<unknown> => ({ consentState: "allowed" });
    const current = conversation(id, () => state());
    const { result } = renderHook(() =>
      useConsentState(client as unknown as Client, current),
    );
    await waitFor(() => {
      expect(result.current).toBe("allowed");
    });

    state = async () => {
      throw new Error("read failed");
    };
    const [, listener] = client.startListener.mock.calls[0];
    // act keeps the state update even though the listener rejects
    await act(async () => {
      await expect(
        listener({
          kind: "consent.changed",
          consent_changed: {
            entity_kind: "conversation",
            entity: id,
            state: "denied",
          },
        }),
      ).rejects.toThrow("read failed");
    });
    expect(result.current).toBeUndefined();
  });
});
