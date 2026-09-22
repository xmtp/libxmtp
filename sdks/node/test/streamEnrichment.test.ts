import type {
  Conversation as NativeConversation,
  Conversations as NativeConversations,
} from "@xmtp/node-bindings";
import { describe, expect, it, vi } from "vitest";

import type { Client } from "@/Client";
import type { CodecRegistry } from "@/CodecRegistry";
import { Conversation } from "@/Conversation";
import { Conversations } from "@/Conversations";
import type { MessageAcknowledgement } from "@/MessageStream";

// Only delivery conversion runs here. Native behavior has separate integration tests.
vi.mock("@xmtp/node-bindings", () => ({
  SortDirection: {},
  ConversationType: {},
  GroupMembershipState: {},
  NotificationFailure: {},
  NotificationOverride: {},
  NotificationStateKind: {},
}));

const codec = vi.hoisted(() => ({ decode: vi.fn(), assert: vi.fn() }));
vi.mock("@/DecodedMessage", () => ({
  DecodedMessage: class {
    deliveryCursor?: unknown;
    constructor(_registry: unknown, retained: unknown) {
      codec.decode(retained);
    }
  },
  assertMessageDecodedForDelivery: codec.assert,
}));

const cursor = { databaseId: new Uint8Array(16), deliverySequence: 1n };
type Enriched = NonNullable<
  ReturnType<MessageAcknowledgement["enrichedMessage"]>
>;

const makeStream = async (kind: "conversation" | "all") => {
  const retained = { id: "message-id" } as Enriched;
  const onError = vi.fn();
  const acknowledgement = {
    enrichedMessage: vi.fn<MessageAcknowledgement["enrichedMessage"]>(
      () => retained,
    ),
    checkOwner: vi.fn(() => true),
    acknowledge: vi.fn(),
    reject: vi.fn(),
  };
  const reader = {
    nextDelivery: vi
      .fn()
      .mockResolvedValueOnce({
        message: { id: "message-id" },
        cursor,
        acknowledgement,
      })
      .mockResolvedValue(null),
    close: vi.fn(),
  };
  const native = { messageReader: vi.fn(async () => reader) };
  const legacyLookup = vi.fn(() => undefined);
  const client = {
    conversations: { getMessageById: legacyLookup },
  } as unknown as Client;
  const registry = {} as CodecRegistry;
  const stream =
    kind === "conversation"
      ? await new Conversation(
          client,
          registry,
          native as unknown as NativeConversation,
        ).stream({ onError })
      : await new Conversations(
          client,
          registry,
          native as unknown as NativeConversations,
        ).streamAllMessages({ onError });
  return { stream, reader, acknowledgement, retained, onError, legacyLookup };
};

describe.each(["conversation", "all"] as const)(
  "%s stream enrichment",
  (kind) => {
    it("enriches the retained item before decoding and handing off once", async () => {
      codec.decode.mockReset();
      codec.assert.mockReset();
      const h = await makeStream(kind);
      expect((await h.stream.next()).done).toBe(false);
      expect(h.acknowledgement.enrichedMessage).toHaveBeenCalledOnce();
      expect(h.acknowledgement.acknowledge).not.toHaveBeenCalled();
      expect(codec.decode).toHaveBeenCalledExactlyOnceWith(h.retained);
      expect(codec.assert).toHaveBeenCalledOnce();
      expect(h.reader.nextDelivery).toHaveBeenCalledOnce();
      expect(h.legacyLookup).not.toHaveBeenCalled();
      await h.stream.end();
    });

    it("ends on the first enrichment storage failure without calling a codec", async () => {
      codec.decode.mockReset();
      codec.assert.mockReset();
      const h = await makeStream(kind);
      const cause = new Error(
        "[StorageError::DbConnection] storage unavailable",
      );
      // A repeated call would succeed. The stream must still end on this error.
      h.acknowledgement.enrichedMessage.mockImplementationOnce(() => {
        throw cause;
      });
      await expect(h.stream.next()).rejects.toBe(cause);
      expect(h.onError).toHaveBeenCalledExactlyOnceWith(cause);
      expect(h.acknowledgement.enrichedMessage).toHaveBeenCalledOnce();
      expect(h.acknowledgement.acknowledge).not.toHaveBeenCalled();
      expect(h.acknowledgement.reject).toHaveBeenCalledOnce();
      expect(h.reader.close).toHaveBeenCalledOnce();
      expect(h.reader.nextDelivery).toHaveBeenCalledOnce();
      expect(codec.decode).not.toHaveBeenCalled();
      expect(codec.assert).not.toHaveBeenCalled();
      expect(h.stream.isDone).toBe(true);
    });

    it("keeps codec failure terminal after native enrichment succeeds", async () => {
      codec.decode.mockReset();
      const cause = new Error("application codec failed");
      codec.assert.mockReset().mockImplementation(() => {
        throw cause;
      });
      const h = await makeStream(kind);
      const next = h.stream.next();
      await expect(next).rejects.toBe(cause);
      expect(codec.decode).toHaveBeenCalledOnce();
      expect(h.acknowledgement.enrichedMessage).toHaveBeenCalledOnce();
      expect(h.acknowledgement.acknowledge).not.toHaveBeenCalled();
      expect(h.acknowledgement.reject).toHaveBeenCalledOnce();
      expect(h.reader.close).toHaveBeenCalledOnce();
    });

    it("reselects a native null with invalid ownership without decoding", async () => {
      codec.decode.mockReset();
      codec.assert.mockReset();
      const h = await makeStream(kind);
      h.acknowledgement.checkOwner.mockReturnValue(false);
      h.acknowledgement.enrichedMessage.mockReturnValue(null);
      expect(await h.stream.next()).toEqual({ done: true, value: undefined });
      expect(codec.decode).not.toHaveBeenCalled();
      expect(h.acknowledgement.acknowledge).not.toHaveBeenCalled();
      expect(h.acknowledgement.reject).toHaveBeenCalledOnce();
      expect(h.reader.nextDelivery).toHaveBeenCalledTimes(2);
    });
  },
);
