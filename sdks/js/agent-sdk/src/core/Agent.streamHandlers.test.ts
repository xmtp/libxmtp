import type { Client } from "@xmtp/node-sdk";
import { describe, expect, it, vi } from "vitest";
import { Agent } from "@/core/Agent";

// The stream `onValue`/`onError` handlers are async functions handed to
// node-sdk's void-returning callbacks. If the recovery they trigger rejects
// (e.g. `#handleStreamError` throwing while tearing streams down), that
// rejection must be routed to the agent's `unhandledError` sink — never leaked
// as an unhandled promise rejection that can crash the host process.
describe("Agent stream handler error routing", () => {
  const makeAgent = () => {
    const captured: {
      conversation?: {
        onValue: (value: unknown) => unknown;
        onError: (error: Error) => unknown;
      };
      message?: {
        onValue: (value: unknown) => unknown;
        onError: (error: Error) => unknown;
      };
    } = {};

    // Closing the conversation stream rejects, so `#handleStreamError` throws
    // mid-recovery — the failure path under test.
    const conversationStream = {
      end: vi.fn(() => Promise.reject(new Error("failed to close stream"))),
    };
    const messageStream = { end: vi.fn(() => Promise.resolve()) };

    const client = {
      conversations: {
        stream: vi.fn((options: typeof captured.conversation) => {
          captured.conversation = options;
          return Promise.resolve(conversationStream);
        }),
        streamAllMessages: vi.fn((options: typeof captured.message) => {
          captured.message = options;
          return Promise.resolve(messageStream);
        }),
      },
    } as unknown as Client;

    return { agent: new Agent({ client }), captured };
  };

  it("routes a rejecting conversation onError to unhandledError without leaking", async () => {
    const { agent, captured } = makeAgent();
    const unhandled = vi.fn();
    agent.on("unhandledError", unhandled);

    await agent.start();

    // node-sdk fires stream callbacks fire-and-forget; the handler must return
    // void, never a rejecting promise.
    const returned = captured.conversation!.onError(
      new Error("stream disconnected"),
    );
    await expect(Promise.resolve(returned)).resolves.toBeUndefined();

    await vi.waitFor(() => {
      expect(unhandled).toHaveBeenCalledTimes(1);
    });
    expect(unhandled.mock.calls[0]![0]).toBeInstanceOf(Error);
  });

  it("routes a rejecting message onError to unhandledError without leaking", async () => {
    const { agent, captured } = makeAgent();
    const unhandled = vi.fn();
    agent.on("unhandledError", unhandled);

    await agent.start();

    const returned = captured.message!.onError(
      new Error("stream disconnected"),
    );
    await expect(Promise.resolve(returned)).resolves.toBeUndefined();

    await vi.waitFor(() => {
      expect(unhandled).toHaveBeenCalledTimes(1);
    });
  });
});

// A stopped agent must never leave a resurrected stream alive: if stop() races
// in while recovery is mid re-setup, the freshly created streams must be torn
// down rather than left delivering.
describe("Agent recovery ownership on stop", () => {
  it("tears down streams re-created after stop() instead of leaking a zombie", async () => {
    const captured: {
      conversation?: { onError: (error: Error) => unknown };
    } = {};

    let resolveRecoverySetup!: () => void;
    let conversationStreamCalls = 0;
    const startConversationStream = { end: vi.fn(() => Promise.resolve()) };
    const recoveredConversationStream = { end: vi.fn(() => Promise.resolve()) };
    const messageStream = { end: vi.fn(() => Promise.resolve()) };

    const client = {
      conversations: {
        stream: vi.fn((options: typeof captured.conversation) => {
          captured.conversation = options;
          conversationStreamCalls += 1;
          if (conversationStreamCalls === 1) {
            return Promise.resolve(startConversationStream);
          }
          // Recovery's re-setup hangs until we release it below.
          return new Promise((resolve) => {
            resolveRecoverySetup = () => resolve(recoveredConversationStream);
          });
        }),
        streamAllMessages: vi.fn(() => Promise.resolve(messageStream)),
      },
    } as unknown as Client;

    const agent = new Agent({ client });
    agent.on("unhandledError", () => {});

    await agent.start();

    // Trigger recovery; it re-syncs, re-sets up, and blocks on the new stream.
    captured.conversation!.onError(new Error("stream disconnected"));
    await vi.waitFor(() => {
      expect(conversationStreamCalls).toBe(2);
    });

    // Stop while the re-setup is still in flight.
    await agent.stop();

    // Release the re-setup: the guard must close the resurrected streams.
    resolveRecoverySetup();
    await vi.waitFor(() => {
      expect(recoveredConversationStream.end).toHaveBeenCalled();
    });
  });

  it("tears down a partially set-up stream when recovery setup rejects after stop()", async () => {
    const captured: {
      conversation?: { onError: (error: Error) => unknown };
    } = {};

    let conversationCalls = 0;
    let messageCalls = 0;
    let resolveConversationSetup!: () => void;
    let rejectMessageSetup!: () => void;
    const recoveredConversationStream = { end: vi.fn(() => Promise.resolve()) };
    const okStream = () => ({ end: vi.fn(() => Promise.resolve()) });

    const client = {
      conversations: {
        stream: vi.fn((options: typeof captured.conversation) => {
          captured.conversation = options;
          conversationCalls += 1;
          if (conversationCalls === 1) {
            return Promise.resolve(okStream());
          }
          // Recovery assigns this stream, then hangs on the message stream.
          return new Promise((resolve) => {
            resolveConversationSetup = () =>
              resolve(recoveredConversationStream);
          });
        }),
        streamAllMessages: vi.fn(() => {
          messageCalls += 1;
          if (messageCalls === 1) {
            return Promise.resolve(okStream());
          }
          return new Promise((_resolve, reject) => {
            rejectMessageSetup = () =>
              reject(new Error("message stream setup failed"));
          });
        }),
      },
    } as unknown as Client;

    const agent = new Agent({ client });
    agent.on("unhandledError", () => {});

    await agent.start();

    captured.conversation!.onError(new Error("stream disconnected"));
    await vi.waitFor(() => {
      expect(conversationCalls).toBe(2);
    });

    // Stop before the conversation stream is assigned, then let setup assign it
    // and reject on the message stream.
    await agent.stop();
    resolveConversationSetup();
    await vi.waitFor(() => {
      expect(messageCalls).toBe(2);
    });
    rejectMessageSetup();

    // The partially created stream must be closed even though setup rejected.
    await vi.waitFor(() => {
      expect(recoveredConversationStream.end).toHaveBeenCalled();
    });
  });
});
