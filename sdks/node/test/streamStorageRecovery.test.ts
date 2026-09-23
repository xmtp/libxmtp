import { setTimeout as sleep } from "node:timers/promises";

import { createRegisteredClient, createSigner } from "@test/helpers";
import type { Client as NativeClient } from "@xmtp/node-bindings";
import { describe, expect, it, vi } from "vitest";

import type { Client } from "@/Client";
import type * as ClientFactory from "@/utils/createClient";

const captured = vi.hoisted(() => ({ clients: [] as NativeClient[] }));

// Preserve the real factory and its result. Only record the native handle
// needed to inject a connection fault below the unchanged public SDK.
vi.mock("@/utils/createClient", async (importOriginal) => {
  const actual = await importOriginal<typeof ClientFactory>();
  return {
    ...actual,
    createClient: async (...args: Parameters<typeof actual.createClient>) => {
      const result = await actual.createClient(...args);
      captured.clients.push(result.client);
      return result;
    },
  };
});

const DELIVERY_WAIT = { timeout: 30_000, interval: 50 };
const SOFT_TEST_TIMEOUT = 90_000;
const HARD_TEST_TIMEOUT = 180_000;

describe("public conversation notification storage failure", () => {
  it.each(["setup", "running"] as const)(
    "preserves the native %s error and permits explicit same-client reopen",
    async (phase) => {
      const previousCaptures = new Set(captured.clients);
      const clients: Client[] = [];
      const streams: Array<
        Awaited<ReturnType<Client["conversations"]["streamGroups"]>>
      > = [];
      const notified: string[] = [];
      const expected: string[] = [];
      const errors: Error[] = [];
      const failed = Promise.withResolvers<Error>();
      const onRetry = vi.fn();
      let native: NativeClient | undefined;
      let disconnected = false;
      let stopping = false;
      let restoration: Promise<void> | undefined;
      const reconnect = () => {
        if (restoration) return restoration;
        if (!disconnected || !native) return Promise.resolve();
        restoration = native
          .dbReconnect()
          .then(() => {
            disconnected = false;
          })
          .finally(() => {
            restoration = undefined;
          });
        return restoration;
      };

      try {
        await within(
          (async () => {
            const sender = await createRegisteredClient(createSigner().signer, {
              disableDeviceSync: true,
            });
            clients.push(sender);
            const receiver = await createRegisteredClient(
              createSigner().signer,
              {
                disableDeviceSync: true,
              },
            );
            clients.push(receiver);
            native = captured.clients.find(
              (client) => client.inboxId() === receiver.inboxId,
            );
            if (!native)
              throw new Error("The receiver native client was not captured");
            const receiverNative = native;
            const release = () => {
              if (stopping)
                throw new Error("The notification storage test stopped");
              receiverNative.releaseDbConnection();
              disconnected = true;
            };
            const open = async () => {
              if (stopping)
                throw new Error("The notification storage test stopped");
              const stream = await receiver.conversations.streamGroups({
                // Reach native subscription setup directly. A pre-sync failure
                // would not test callback ordering against waitForReady/on_close.
                disableSync: true,
                onValue: (group) => {
                  notified.push(group.id);
                },
                onError: (error) => {
                  errors.push(error);
                  failed.resolve(error);
                },
                onRetry,
              });
              streams.push(stream);
              if (stopping) await stream.end();
              return stream;
            };
            const createAndReceive = async (
              stream: (typeof streams)[number],
            ) => {
              if (stopping)
                throw new Error("The notification storage test stopped");
              const delivery = stream.next();
              // Handle a rejection immediately if native failure wins the race
              // with publishing the new group's Welcome.
              void delivery.catch(() => {});
              const group = await sender.conversations.createGroup([
                receiver.inboxId,
              ]);
              const result = await within(
                delivery,
                DELIVERY_WAIT.timeout,
                "The new group notification did not arrive",
              );
              expect(result.done).toBe(false);
              expect(result.value?.id).toBe(group.id);
              expected.push(group.id);
              expect(notified).toEqual(expected);
            };

            if (phase === "setup") release();
            const stream = await open();
            if (phase === "running") await createAndReceive(stream);
            const rejected = stream.next().then(
              (result) => ({ result, error: undefined }),
              (error: unknown) => ({ result: undefined, error }),
            );
            if (phase === "running") release();
            const cause = await within(
              failed.promise,
              DELIVERY_WAIT.timeout,
              "The native storage error did not reach onError",
            );
            expect(cause.message).toMatch(
              /^\[SubscribeError::(?:Db|Storage)\]/,
            );
            expect(
              (
                await within(
                  rejected,
                  DELIVERY_WAIT.timeout,
                  "The notification iterator did not reject",
                )
              ).error,
            ).toBe(cause);
            expect(disconnected).toBe(true);
            expect(stream.isDone).toBe(true);
            expect(errors).toEqual([cause]);
            expect(onRetry).not.toHaveBeenCalled();

            await reconnect();
            // The failed iterator reports its cause once, then stays at EOF.
            await expect(stream.next()).resolves.toEqual({
              done: true,
              value: undefined,
            });
            const replacement = await open();
            await createAndReceive(replacement);
            await stream.end();
            await createAndReceive(replacement);
            expect(replacement.isDone).toBe(false);
            expect(stream.isDone).toBe(true);
            expect(errors).toEqual([cause]);
            expect(onRetry).not.toHaveBeenCalled();
          })(),
          SOFT_TEST_TIMEOUT,
          "The notification storage test deadline expired",
        );
      } finally {
        stopping = true;
        try {
          await within(
            reconnect(),
            5_000,
            "The notification DB cleanup deadline expired",
          );
        } finally {
          try {
            await within(
              Promise.allSettled(streams.map((stream) => stream.end())),
              5_000,
              "The notification stream cleanup deadline expired",
            );
          } finally {
            try {
              await within(
                Promise.allSettled(clients.map((client) => client.close())),
                5_000,
                "The notification client cleanup deadline expired",
              );
            } finally {
              captured.clients = captured.clients.filter((client) =>
                previousCaptures.has(client),
              );
            }
          }
        }
      }
    },
    HARD_TEST_TIMEOUT,
  );
});

type PublicStream = Awaited<
  ReturnType<Client["conversations"]["streamAllMessages"]>
>;

async function within<T>(
  operation: Promise<T>,
  timeout: number,
  label: string,
) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      operation,
      new Promise<never>((_, reject) => {
        timer = setTimeout(() => reject(new Error(label)), timeout);
      }),
    ]);
  } finally {
    if (timer !== undefined) clearTimeout(timer);
  }
}

describe("public message stream storage failure", () => {
  it.each(["callback", "iterator"] as const)(
    "ends on a DB fault and replays an unacknowledged item after explicit reopen in %s mode",
    async (mode) => {
      const previousCaptures = new Set(captured.clients);
      const clients: Client[] = [];
      const streams: PublicStream[] = [];
      const peerErrors: unknown[] = [];
      const receiverErrors: Error[] = [];
      const received: number[] = [];
      const deliveryIds: string[] = [];
      const replies: number[] = [];
      const released = Promise.withResolvers<undefined>();
      const failed = Promise.withResolvers<Error>();
      let native: NativeClient | undefined;
      let disconnected = false;
      let injected = false;
      let stopping = false;
      let restoration: Promise<void> | undefined;
      const consumptions: Promise<void>[] = [];
      let iteratorFailure: unknown;

      const reconnect = () => {
        if (restoration) return restoration;
        if (!disconnected || !native) return Promise.resolve();
        restoration = native
          .dbReconnect()
          .then(() => {
            disconnected = false;
          })
          .finally(() => {
            restoration = undefined;
          });
        return restoration;
      };

      try {
        const sender = await createRegisteredClient(createSigner().signer, {
          disableDeviceSync: true,
        });
        clients.push(sender);
        const receiver = await createRegisteredClient(createSigner().signer, {
          disableDeviceSync: true,
        });
        clients.push(receiver);
        native = captured.clients.find(
          (client) => client.inboxId() === receiver.inboxId,
        );
        if (!native)
          throw new Error("The receiver native client was not captured");
        const receiverNative = native;
        const group = await sender.conversations.createGroup([
          receiver.inboxId,
        ]);
        await group.sendText("storage-request:0");
        await group.sendText("storage-request:1");
        // Preload both requests. The first callback completes before its
        // acknowledgement fails; reopening must replay that retained item.
        await receiver.conversations.sync();
        const receiverGroup = await receiver.conversations.getConversationById(
          group.id,
        );
        if (!receiverGroup) throw new Error("The receiver group is missing");
        await receiverGroup.sync();

        const peer = await sender.conversations.streamAllMessages({
          onValue: (message) => {
            if (
              typeof message.content === "string" &&
              message.content.startsWith("storage-reply:")
            ) {
              replies.push(Number(message.content.split(":")[1]));
            }
          },
          onError: (error) => peerErrors.push(error),
        });
        streams.push(peer);

        const handle: NonNullable<
          Parameters<typeof receiver.conversations.streamAllMessages>[0]
        >["onValue"] = async (message) => {
          if (
            stopping ||
            typeof message.content !== "string" ||
            !message.content.startsWith("storage-request:")
          )
            return;
          if (disconnected)
            throw new Error(
              "A message was handed off before storage recovered",
            );
          const sequence = Number(message.content.split(":")[1]);
          await receiverGroup.sendText(`storage-reply:${sequence}`);
          if (stopping) return;
          received.push(sequence);
          deliveryIds.push(message.id);
          if (sequence === 0 && !injected) {
            injected = true;
            receiverNative.releaseDbConnection();
            disconnected = true;
            // The handler returns successfully while persistence is unavailable.
            // The caller repairs storage only after this stream reports failure.
            released.resolve(undefined);
          }
        };

        const open = async () => {
          const stream = await receiver.conversations.streamAllMessages({
            ...(mode === "callback" ? { onValue: handle } : {}),
            onError: (error) => {
              receiverErrors.push(error);
              failed.resolve(error);
            },
          });
          streams.push(stream);
          if (mode === "iterator") {
            consumptions.push(
              (async () => {
                for await (const message of stream) await handle(message);
              })().catch((error: unknown) => {
                iteratorFailure = error;
              }),
            );
          }
          return stream;
        };
        const stream = await open();

        await within(
          (async () => {
            await released.promise;
            const cause = await within(
              failed.promise,
              DELIVERY_WAIT.timeout,
              "The DB failure did not terminate the stream",
            );
            expect(disconnected).toBe(true);
            expect(received).toEqual([0]);
            expect(receiverErrors).toEqual([cause]);
            expect(peerErrors).toEqual([]);
            expect(stream.isDone).toBe(true);
            if (mode === "iterator") {
              await consumptions[0];
              expect(iteratorFailure).toBe(cause);
            }
            await reconnect();
            expect(disconnected).toBe(false);
            // Repair alone does not restart a terminal stream.
            await sleep(100);
            expect(stream.isDone).toBe(true);
            expect(received).toEqual([0]);
            const replacement = await open();
            await expect.poll(() => replies, DELIVERY_WAIT).toEqual([0, 0, 1]);
            expect(received).toEqual([0, 0, 1]);
            expect(deliveryIds[1]).toBe(deliveryIds[0]);
            expect(deliveryIds[2]).not.toBe(deliveryIds[0]);
            // Ending the old handle again must not close the new reader.
            await stream.end();
            if (stopping)
              throw new Error("The storage test stopped before the final send");
            await group.sendText("storage-request:2");
            await expect
              .poll(() => replies, DELIVERY_WAIT)
              .toEqual([0, 0, 1, 2]);
            expect(received).toEqual([0, 0, 1, 2]);
            expect(replacement.isDone).toBe(false);
            expect(receiverErrors).toEqual([cause]);
            expect(peerErrors).toEqual([]);
          })(),
          SOFT_TEST_TIMEOUT,
          "The storage failure test deadline expired",
        );
      } finally {
        stopping = true;
        // Restore before closing, including the soft-timeout path. Always close
        // the remaining resources if reconnect itself fails.
        try {
          await within(reconnect(), 5_000, "The DB cleanup deadline expired");
        } finally {
          try {
            await within(
              Promise.allSettled(streams.map((stream) => stream.end())),
              5_000,
              "The stream cleanup deadline expired",
            );
            await within(
              Promise.allSettled(consumptions),
              5_000,
              "The iterator cleanup deadline expired",
            );
          } finally {
            try {
              await within(
                Promise.allSettled(clients.map((client) => client.close())),
                5_000,
                "The client cleanup deadline expired",
              );
            } finally {
              captured.clients = captured.clients.filter((client) =>
                previousCaptures.has(client),
              );
            }
          }
        }
      }
    },
    HARD_TEST_TIMEOUT,
  );
});
