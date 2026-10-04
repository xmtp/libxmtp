import { randomUUID } from "node:crypto";
import { setTimeout as sleep } from "node:timers/promises";

import { createRegisteredClient, createSigner } from "@test/helpers";
import { createRecoveryBackend } from "@test/recoveryBackend";
import { createRecoveryProxy } from "@test/recoveryProxy";
import {
  Group,
  MessageStream,
  XmtpError,
  flushTelemetry,
  initLogging,
  type Client,
  type Message,
  type ConnectionState,
  type MessageReaderOptions,
} from "@xmtp/node-sdk";
import { afterAll, describe, expect, it } from "vitest";

const GROUP_COUNT = 8;
const DELIVERY_WAIT = { timeout: 60_000, interval: 100 };
// Production wire silence can take three 30-second keepalive intervals.
const RECOVERY_WAIT = { timeout: 180_000, interval: 100 };
const TEST_TIMEOUT = 660_000;
// Core requires 30 seconds of registered health before resetting its budget.
const HEALTHY_INTERVAL_MS = 32_000;
const traceEndpoint = process.env.XMTP_RECOVERY_TRACE_ENDPOINT;
const traceRun = process.env.XMTP_RECOVERY_TRACE_RUN;
// Logging is process global. The first client sets these attributes for all
// clients in this process; the run label does not identify a single client.
if (traceEndpoint)
  await initLogging({
    level: process.env.XMTP_RECOVERY_TRACE_VERBOSE === "1" ? "trace" : "info",
    otel: {
      endpoint: traceEndpoint,
      serviceName: "xmtp-sdk-recovery",
      sampleRatio: 1,
    },
    resourceAttributes: traceRun
      ? new Map([["xmtp.recovery.run", traceRun]])
      : undefined,
  });

afterAll(async () => {
  if (traceEndpoint) await flushTelemetry();
}, 30_000);

type PublicStream = MessageStream;
const connectionStates = new WeakMap<
  MessageStream,
  { connection?: ConnectionState; connectionGeneration: number }
>();
const streamStateSnapshot = (stream: MessageStream) => ({
  current: { ...connectionStates.get(stream)! },
});
async function observeMessages(
  client: Client,
  observation: {
    consume?: (message: Message) => void | Promise<void>;
    failure: (error: Error) => void;
    from?: MessageReaderOptions["from"];
  },
): Promise<MessageStream> {
  const state: { connection?: ConnectionState; connectionGeneration: number } =
    { connectionGeneration: 0 };
  const stream = MessageStream.open(
    client,
    { from: observation.from },
    {
      onConnectionStateChange: (_previous, current) => {
        state.connection = current;
        state.connectionGeneration++;
      },
      onClose: (reason) => {
        if (reason.kind === "failed")
          observation.failure(reason.error as Error);
      },
    },
  );
  connectionStates.set(stream, state);
  await stream.ready();
  if (observation.consume)
    void stream.onValue(observation.consume).catch(() => {});
  return stream;
}

async function expectCommonGroupState(
  sender: Pick<Group, "id" | "debugInfo" | "members">,
  receiver: Client,
  replyId: string,
) {
  const peer = await receiver.conversations.getById(sender.id);
  expect(peer).toBeInstanceOf(Group);
  if (!(peer instanceof Group)) throw new Error("The peer group is missing");
  expect((await receiver.conversations.getMessageById(replyId))?.id).toBe(
    replyId,
  );
  const [sent, received] = await Promise.all([
    sender.debugInfo(),
    peer.debugInfo(),
  ]);
  // Both clients have the exact final reply. Equal cursors compare the same
  // processed prefix; a sampled network head alone cannot establish this.
  expect(received.cursor).toEqual(sent.cursor);
  expect(received.epoch).toBe(sent.epoch);
  expect(received.maybeForked).toBe(false);
  expect(sent.maybeForked).toBe(false);
  const members = async (group: Pick<Group, "members">) =>
    (await group.members())
      .map((member) => ({
        inboxId: member.inboxId,
        identities: member.identities,
        permissionLevel: member.permissionLevel,
      }))
      .sort((a, b) => a.inboxId.localeCompare(b.inboxId));
  expect(await members(peer)).toEqual(await members(sender));
}

describe("public message stream recovery", () => {
  const drainLabel = process.env.XMTP_RECOVERY_BACKEND_BINARY
    ? "graceful backend restart"
    : "TCP EOF";
  it.each([
    ["callback", "disconnect"],
    ["iterator", "disconnect"],
    ["callback", "blackhole-inbound"],
    ["iterator", "blackhole-inbound"],
    ["callback", "blackhole-outbound"],
    ["iterator", "blackhole-outbound"],
    ["callback", "drain"],
    ["iterator", "drain"],
  ] as const)(
    `receives and replies through state changes in %s mode after %s (drain: ${drainLabel})`,
    async (mode, fault) => {
      const backend =
        fault === "drain" && process.env.XMTP_RECOVERY_BACKEND_BINARY
          ? await createRecoveryBackend()
          : undefined;
      const proxy = await createRecoveryProxy(backend?.url).catch(
        async (error: unknown) => {
          await backend?.close();
          throw error;
        },
      );
      const clients: Client[] = [];
      const streams: PublicStream[] = [];
      let consumption: Promise<void> | undefined;
      const started: string[] = [];
      const received: string[] = [];
      const replies: string[] = [];
      const replyIds = new Map<string, string>();
      const peerReplyIds = new Map<string, string>();
      const installationReceived: string[] = [];
      const joinedReceived: string[] = [];
      const errors: unknown[] = [];
      let installation: Client | undefined;
      // Short live repetition complements the twelve-cycle controlled-clock
      // Core regression. Each drain cycle uses the same public stream handle.
      const cycles = fault === "drain" ? 3 : 1;
      const finalStage = cycles * 2;
      try {
        const sender = await createRegisteredClient(createSigner().signer, {
          deviceSync: false,
        });
        clients.push(sender);
        const receiverSigner = createSigner().signer;
        const receiver = await createRegisteredClient(receiverSigner, {
          backend: { url: proxy.url },
          deviceSync: false,
        });
        clients.push(receiver);
        const joining = await createRegisteredClient(createSigner().signer, {
          deviceSync: false,
        });
        clients.push(joining);
        const groups: Array<
          Awaited<ReturnType<typeof sender.conversations.createGroup>>
        > = [];
        for (let index = 0; index < GROUP_COUNT; index += 1) {
          groups.push(
            await sender.conversations.createGroup([receiver.inboxId]),
          );
        }
        // Initial setup only. Recovery below does not sync or reopen the receiver.
        await receiver.conversations.sync();
        const handle: (message: Message) => Promise<void> = async (message) => {
          if (
            message.content.kind !== "text" ||
            !message.content.value.startsWith("request:")
          )
            return;
          started.push(message.content.value);
          const group = await receiver.conversations.getById(
            message.conversationId,
          );
          if (!group) throw new Error("The delivered group is missing");
          const reply = `reply:${message.content.value}`;
          replyIds.set(reply, await group.sendText(reply));
          received.push(message.content.value);
        };
        const observeRequests = async (client: Client, values: string[]) => {
          const observer = await observeMessages(client, {
            consume: (message) => {
              if (
                message.content.kind === "text" &&
                message.content.value.startsWith("request:")
              )
                values.push(message.content.value);
            },
            failure: (error) => errors.push(error),
          });
          streams.push(observer);
        };
        const peerStream = await observeMessages(sender, {
          consume: (message) => {
            if (
              message.content.kind === "text" &&
              message.content.value.startsWith("reply:")
            ) {
              replies.push(message.content.value);
              peerReplyIds.set(message.content.value, message.id);
            }
          },
          failure: (error) => errors.push(error),
        });
        streams.push(peerStream);
        const stream = await observeMessages(receiver, {
          ...(mode === "callback" ? { consume: handle } : {}),
          failure: (error) => errors.push(error),
        });
        streams.push(stream);
        if (mode === "iterator") {
          consumption = (async () => {
            for await (const message of stream) await handle(message);
          })().catch((error: unknown) => {
            errors.push(error);
          });
        }
        const sendStage = async (stage: number) => {
          for (const [index, group] of groups.entries())
            await group.sendText(`request:${index}:${stage}`);
        };
        await sendStage(0);
        await expect
          .poll(() => replies.length, DELIVERY_WAIT)
          .toBe(GROUP_COUNT);
        await expect
          .poll(() => received.length, DELIVERY_WAIT)
          .toBe(GROUP_COUNT);
        for (let cycle = 0; cycle < cycles; cycle += 1) {
          const prior = streamStateSnapshot(stream).current;
          if (backend) await backend.stopGracefully();
          else proxy.inject(fault);
          if (cycle === 0) {
            // Register through the healthy endpoint while the original
            // installation stays offline on its existing client and stream.
            installation = await createRegisteredClient(receiverSigner, {
              storage: {
                location: {
                  dbPath: `./test-${randomUUID()}.db3`,
                  attachmentsDir: `./test-attachments-${randomUUID()}`,
                },
              },
              deviceSync: false,
            });
            clients.push(installation);
            expect(installation.inboxId).toBe(receiver.inboxId);
            expect(installation.installationId).not.toBe(
              receiver.installationId,
            );
            await observeRequests(installation, installationReceived);
            await observeRequests(joining, joinedReceived);
            await groups[0].updateName("changed during outage");
            await groups[1].addMembers([joining.inboxId]);
            await groups[2].removeMembers([receiver.inboxId]);
            // Replacing the removed group keeps eight eligible active groups.
            groups.push(
              await sender.conversations.createGroup([receiver.inboxId]),
            );
            // Send-triggered installation refresh has a five-second cadence.
            await sleep(6_000);
          }
          const during = cycle * 2 + 1;
          await sendStage(during);
          if (backend) {
            expect(received).toHaveLength(GROUP_COUNT * during);
            // A state callback can take longer than the recovery budget.
            // Restart the drained backend before waiting for that callback.
            await backend.start();
          }
          await expect
            .poll(async () => {
              const current = streamStateSnapshot(stream).current;
              return (
                current.connection !== "connected" ||
                current.connectionGeneration > prior.connectionGeneration
              );
            }, RECOVERY_WAIT)
            .toBe(true);
          if (!backend) {
            expect(received).toHaveLength(GROUP_COUNT * during);
            proxy.restore();
          }
          try {
            await expect
              .poll(() => replies.length, RECOVERY_WAIT)
              .toBe(GROUP_COUNT * (during + 1));
          } catch (cause) {
            const snapshot = await within(
              Promise.resolve(streamStateSnapshot(stream)),
              5_000,
              "The failed stream snapshot was unavailable",
            ).catch(() => undefined);
            // These values contain only test messages and public group IDs.
            // Keep enough state to find the missing operation in Tempo.
            throw new Error(
              JSON.stringify(
                {
                  mode,
                  fault,
                  groups: groups.map((group, index) => ({
                    index,
                    id: group.id,
                  })),
                  started,
                  received,
                  replies,
                  snapshot,
                  errorCodes: errors.map((error) =>
                    error instanceof Error
                      ? (error.message.match(/^\[[^\]]+\]/)?.[0] ?? error.name)
                      : typeof error,
                  ),
                },
                (_, value: unknown) =>
                  value instanceof Uint8Array
                    ? Buffer.from(value).toString("hex")
                    : typeof value === "bigint"
                      ? value.toString()
                      : value,
              ),
              { cause },
            );
          }
          await sendStage(during + 1);
          await expect
            .poll(() => replies.length, DELIVERY_WAIT)
            .toBe(GROUP_COUNT * (during + 2));
          await expect
            .poll(() => received.length, DELIVERY_WAIT)
            .toBe(GROUP_COUNT * (during + 2));
          await expect(stream.ready()).resolves.toBeUndefined();
          expect(errors).toEqual([]);
          if (cycle + 1 < cycles) {
            // Quiet groups must stay healthy without synthetic application
            // messages. Start the production reset interval after the
            // receiver reports a connected receipt source.
            await expect
              .poll(
                () => streamStateSnapshot(stream).current.connection,
                RECOVERY_WAIT,
              )
              .toBe("connected");
            const generation =
              streamStateSnapshot(stream).current.connectionGeneration;
            await sleep(HEALTHY_INTERVAL_MS);
            const healthy = streamStateSnapshot(stream).current;
            expect(healthy.connection).toBe("connected");
            expect(healthy.connectionGeneration).toBe(generation);
          }
        }
        await expect
          .poll(() => installationReceived.length, RECOVERY_WAIT)
          .toBe(GROUP_COUNT * finalStage);
        await expect
          .poll(() => joinedReceived.length, RECOVERY_WAIT)
          .toBe(finalStage);
        await expect
          .poll(() => received.length, DELIVERY_WAIT)
          .toBe(GROUP_COUNT * (finalStage + 1));
        for (let index = 0; index < groups.length; index += 1) {
          const stages =
            index === 2
              ? [0]
              : Array.from(
                  { length: finalStage + 1 },
                  (_, stage) => stage,
                ).filter((stage) => index < GROUP_COUNT || stage > 0);
          const expected = stages.map((stage) => `request:${index}:${stage}`);
          expect(
            received.filter((value) => value.startsWith(`request:${index}:`)),
          ).toEqual(expected);
          expect(
            replies.filter((value) =>
              value.startsWith(`reply:request:${index}:`),
            ),
          ).toEqual(expected.map((value) => `reply:${value}`));
          expect(
            installationReceived.filter((value) =>
              value.startsWith(`request:${index}:`),
            ),
          ).toEqual(expected.filter((value) => !value.endsWith(":0")));
        }
        expect(joinedReceived).toEqual(
          Array.from(
            { length: finalStage },
            (_, index) => `request:1:${index + 1}`,
          ),
        );
        expect(peerReplyIds).toEqual(replyIds);
        for (const [index, group] of groups.entries()) {
          if (index === 2) continue; // This installation was removed.
          const replyId = replyIds.get(`reply:request:${index}:${finalStage}`)!;
          expect((await sender.conversations.getMessageById(replyId))?.id).toBe(
            replyId,
          );
          await expectCommonGroupState(group, receiver, replyId);
        }
        const changed = await receiver.conversations.getById(groups[0].id);
        expect(changed).toBeInstanceOf(Group);
        if (!(changed instanceof Group))
          throw new Error("The changed group is missing");
        expect((await changed.state()).name).toBe("changed during outage");
        if (!installation)
          throw new Error("The new installation was not registered");
        for (const group of [groups[0], changed]) {
          const receiverMember = (await group.members()).find(
            (member) => member.inboxId === receiver.inboxId,
          );
          expect(receiverMember).toBeDefined();
          expect(
            (await receiver.inboxState(true)).installations.map(
              (item) => item.id,
            ),
          ).toContain(installation.installationId);
        }
        const expanded = await receiver.conversations.getById(groups[1].id);
        const removed = await receiver.conversations.getById(groups[2].id);
        expect(
          (await expanded?.members())?.map((member) => member.inboxId),
        ).toContain(joining.inboxId);
        expect(
          (await removed?.members())?.map((member) => member.inboxId),
        ).not.toContain(receiver.inboxId);
        expect(errors).toEqual([]);
      } finally {
        proxy.restore();
        await Promise.allSettled(streams.map((stream) => stream.end()));
        await consumption;
        await Promise.allSettled(clients.map((client) => client.end()));
        try {
          await proxy.close();
        } finally {
          await backend?.close();
        }
      }
    },
    TEST_TIMEOUT,
  );
});

// These deadlines leave room for the real ten-minute native outage budget.
// Repeated connection failures normally reach the ten-attempt limit sooner.
const EXHAUSTION_WAIT_MS = 660_000;
const EXHAUSTION_TEST_MS = 1_560_000;
const isExhausted = (error: Error) =>
  error instanceof XmtpError.RecoveryExhausted;
// Each budget test waits for the real outage budget two times, about nine
// minutes. CI does not run them. See sdks/node/AGENTS.md for when to run them.
const BUDGET_TESTS = process.env.XMTP_RECOVERY_BUDGET_TESTS === "1";

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
    clearTimeout(timer);
  }
}

describe.runIf(BUDGET_TESTS)("public message stream recovery budget", () => {
  it.each(["callback", "iterator"] as const)(
    "exhausts twice and opens fresh streams on the same offline client in %s mode",
    async (mode) => {
      const proxy = await createRecoveryProxy();
      const clients: Client[] = [];
      const streams: PublicStream[] = [];
      const opening = new Set<Promise<PublicStream>>();
      const consuming: Promise<void>[] = [];
      const received: string[] = [];
      const replies: string[] = [];
      const replyIds = new Map<string, string>();
      const peerReplyIds = new Map<string, string>();
      const unexpected = Promise.withResolvers<never>();
      // A failure can arrive between two waits. Keep that rejection handled.
      void unexpected.promise.catch(() => {});
      const reportUnexpected = (error: unknown) => unexpected.reject(error);
      const baselineAcknowledged = Promise.withResolvers<undefined>();
      const records = Array.from({ length: 3 }, () => ({
        ready: Promise.withResolvers<PublicStream>(),
        failed: Promise.withResolvers<Error>(),
        errors: [] as Error[],
        openedAt: 0,
        dropsAtOpen: 0,
        failedAt: 0,
        dropsAtFailure: 0,
        stream: undefined as PublicStream | undefined,
      }));
      let stopping = false;
      let faultStartedDrops = 0;
      const testDeadline = setTimeout(
        () => reportUnexpected(new Error("The recovery budget test timed out")),
        EXHAUSTION_TEST_MS - 30_000,
      );
      const waitFor = <T>(
        operation: Promise<T>,
        timeout: number,
        label: string,
      ) =>
        within(Promise.race([operation, unexpected.promise]), timeout, label);
      try {
        const sender = await waitFor(
          createRegisteredClient(createSigner().signer, {
            deviceSync: false,
          }),
          DELIVERY_WAIT.timeout,
          "The sender did not register",
        );
        clients.push(sender);
        const receiver = await waitFor(
          createRegisteredClient(createSigner().signer, {
            backend: { url: proxy.url },
            deviceSync: false,
          }),
          DELIVERY_WAIT.timeout,
          "The receiver did not register",
        );
        clients.push(receiver);
        const groups: Array<
          Awaited<ReturnType<typeof sender.conversations.createGroup>>
        > = [];
        for (let index = 0; index < GROUP_COUNT; index += 1) {
          groups.push(
            await waitFor(
              sender.conversations.createGroup([receiver.inboxId]),
              DELIVERY_WAIT.timeout,
              `Group ${index} was not created`,
            ),
          );
        }
        await waitFor(
          receiver.conversations.sync(),
          DELIVERY_WAIT.timeout,
          "The initial groups did not sync",
        );
        streams.push(
          await waitFor(
            observeMessages(sender, {
              consume: (message) => {
                if (
                  message.content.kind === "text" &&
                  message.content.value.startsWith("budget-reply:")
                ) {
                  replies.push(message.content.value);
                  peerReplyIds.set(message.content.value, message.id);
                }
              },
              failure: reportUnexpected,
            }),
            DELIVERY_WAIT.timeout,
            "The reply stream did not open",
          ),
        );
        const pendingBaselineReplies = new Set(
          groups.map((_, index) => `budget-reply:${index}:0`),
        );
        const handle: (message: Message) => Promise<void> = async (message) => {
          if (message.content.kind !== "text") return;
          if (pendingBaselineReplies.delete(message.content.value)) {
            // A later item in each group proves all initial requests crossed
            // the durable acknowledgement boundary before the network fault.
            if (pendingBaselineReplies.size === 0)
              baselineAcknowledged.resolve(undefined);
          }
          if (!message.content.value.startsWith("budget-request:")) return;
          received.push(message.content.value);
          const conversation = await receiver.conversations.getById(
            message.conversationId,
          );
          if (!conversation) throw new Error("The delivered group is missing");
          const reply = message.content.value.replace(
            "budget-request:",
            "budget-reply:",
          );
          replyIds.set(reply, await conversation.sendText(reply));
        };
        const open = (generation: number): Promise<PublicStream> => {
          const record = records[generation];
          record.openedAt = performance.now();
          record.dropsAtOpen = proxy.flapDrops;
          const operation = (async () => {
            const stream = await observeMessages(receiver, {
              ...(mode === "callback" ? { consume: handle } : {}),
              failure: (error) => {
                record.errors.push(error);
                record.failedAt = performance.now();
                record.dropsAtFailure = proxy.flapDrops;
                record.failed.resolve(error);
                if (stopping) return;
                if (!isExhausted(error) || generation === 2) {
                  reportUnexpected(error);
                  return;
                }
                if (mode === "callback") {
                  // Open directly from onError, before the failed handler
                  // returns and while the network fault is still active.
                  void open(generation + 1).catch(reportUnexpected);
                }
              },
            });
            streams.push(stream);
            record.stream = stream;
            record.ready.resolve(stream);
            if (stopping) await stream.end();
            else if (mode === "iterator") {
              consuming.push(
                (async () => {
                  try {
                    for await (const message of stream) await handle(message);
                    if (!stopping)
                      throw new Error("The iterator ended without its error");
                  } catch (error) {
                    if (stopping) return;
                    expect(error).toBe(record.errors[0]);
                    if (
                      !(error instanceof Error) ||
                      !isExhausted(error) ||
                      generation === 2
                    )
                      throw error;
                    // The rejected iterator is terminal. Its replacement gets
                    // a separate budget on this same client, still offline.
                    await open(generation + 1);
                  }
                })().catch(reportUnexpected),
              );
            }
            return stream;
          })();
          opening.add(operation);
          void operation.then(
            () => opening.delete(operation),
            () => opening.delete(operation),
          );
          return operation;
        };
        const send = (number: number) =>
          waitFor(
            Promise.all(
              groups.map((group, index) =>
                group.sendText(`budget-request:${index}:${number}`),
              ),
            ),
            DELIVERY_WAIT.timeout,
            `Request ${number} did not publish in all groups`,
          );
        await waitFor(
          open(0),
          DELIVERY_WAIT.timeout,
          "The initial stream did not open",
        );
        await send(0);
        await waitFor(
          baselineAcknowledged.promise,
          DELIVERY_WAIT.timeout,
          "The initial requests were not acknowledged in all groups",
        );
        faultStartedDrops = proxy.flapDrops;
        proxy.inject("flap");
        for (let generation = 0; generation < 2; generation += 1) {
          const record = records[generation];
          const error = await waitFor(
            record.failed.promise,
            EXHAUSTION_WAIT_MS,
            `Stream ${generation} did not exhaust its native budget`,
          );
          expect(error).toBeInstanceOf(XmtpError.RecoveryExhausted);
          expect(error).toMatchObject({
            details: { code: "RecoveryExhausted", category: "stream" },
          });
          expect(error.message).toMatch(/after \d+ attempts/);
          const attempts = Number(
            /after (\d+) attempts/.exec(error.message)?.[1],
          );
          // Either production bound may terminate the stream. The short path
          // must report exactly this stream's ten failed recovery cycles.
          // Core's new_stream_has_fresh_budget_during_the_same_outage test
          // checks that a replacement survives nine new failures. Socket
          // drops prove new transport work, not a native attempt count.
          if (record.failedAt - record.openedAt < 600_000)
            expect(attempts).toBe(10);
          expect(record.dropsAtFailure).toBeGreaterThan(
            generation === 0 ? faultStartedDrops : record.dropsAtOpen,
          );
          expect(record.errors).toHaveLength(1);
          await expect(record.stream!.next()).rejects.toBe(record.errors[0]);
          await waitFor(
            records[generation + 1].ready.promise,
            DELIVERY_WAIT.timeout,
            "The replacement stream did not open while offline",
          );
          if (generation === 0) {
            // Replay has independent delivery progress and can read retained
            // work while the replacement default consumer owns its lease.
            const replayed: string[] = [];
            const replay = await waitFor(
              observeMessages(receiver, {
                from: await receiver.conversations.beginningDeliveryCursor(),
                consume: (message) => {
                  if (
                    message.content.kind === "text" &&
                    message.content.value.startsWith("budget-request:")
                  )
                    replayed.push(message.content.value);
                },
                failure: reportUnexpected,
              }),
              DELIVERY_WAIT.timeout,
              "The independent replay reader did not open",
            );
            streams.push(replay);
            await waitFor(
              expect
                .poll(() => replayed.length, DELIVERY_WAIT)
                .toBe(GROUP_COUNT),
              DELIVERY_WAIT.timeout + 1_000,
              "Stream exhaustion blocked independent local delivery",
            );
            expect(replayed.slice().sort((a, b) => a.localeCompare(b))).toEqual(
              groups.map((_, index) => `budget-request:${index}:0`),
            );
            await replay.end();
            await expect(records[1].stream!.ready()).resolves.toBeUndefined();
          }
        }
        const replacement = await records[2].ready.promise;
        // Give stale callbacks and cleanup a chance to run against generation
        // two before restoring the network. They must not close its reader.
        await sleep(500);
        await expect(replacement.ready()).resolves.toBeUndefined();
        expect(records[2].errors).toEqual([]);
        proxy.inject("disconnect");
        await send(1);
        await send(2);
        proxy.restore();
        await waitFor(
          expect
            .poll(() => replies.length, RECOVERY_WAIT)
            .toBe(GROUP_COUNT * 3),
          RECOVERY_WAIT.timeout + 1_000,
          "The replacement stream did not catch up and reply",
        );
        await waitFor(
          Promise.all([records[0].stream?.end(), records[1].stream?.end()]),
          DELIVERY_WAIT.timeout,
          "The old streams did not close",
        );
        await send(3);
        await waitFor(
          expect
            .poll(() => replies.length, DELIVERY_WAIT)
            .toBe(GROUP_COUNT * 4),
          DELIVERY_WAIT.timeout + 1_000,
          "Cleanup of an old stream stopped the replacement",
        );
        expect(received).toHaveLength(GROUP_COUNT * 4);
        expect(replies).toHaveLength(GROUP_COUNT * 4);
        for (let index = 0; index < GROUP_COUNT; index += 1) {
          expect(
            received.filter((value) =>
              value.startsWith(`budget-request:${index}:`),
            ),
          ).toEqual([0, 1, 2, 3].map((n) => `budget-request:${index}:${n}`));
          expect(
            replies.filter((value) =>
              value.startsWith(`budget-reply:${index}:`),
            ),
          ).toEqual([0, 1, 2, 3].map((n) => `budget-reply:${index}:${n}`));
        }
        expect(records.map((record) => record.errors.length)).toEqual([
          1, 1, 0,
        ]);
        await expect
          .poll(() => replyIds.size, DELIVERY_WAIT)
          .toBe(GROUP_COUNT * 4);
        expect(peerReplyIds).toEqual(replyIds);
        for (const [index, group] of groups.entries()) {
          const replyId = replyIds.get(`budget-reply:${index}:3`)!;
          expect((await sender.conversations.getMessageById(replyId))?.id).toBe(
            replyId,
          );
          await expectCommonGroupState(group, receiver, replyId);
        }
        await expect(replacement.ready()).resolves.toBeUndefined();
      } finally {
        stopping = true;
        clearTimeout(testDeadline);
        proxy.restore();
        // Soft deadlines leave time to close the owned sockets and clients
        // before the test runner's hard timeout can interrupt this cleanup.
        await within(
          Promise.allSettled(opening),
          5_000,
          "Stream creation did not stop",
        ).catch(() => {});
        await within(
          Promise.allSettled(streams.map((stream) => stream.end())),
          5_000,
          "Streams did not close",
        ).catch(() => {});
        await within(
          Promise.allSettled(consuming),
          5_000,
          "Iterator consumers did not stop",
        ).catch(() => {});
        await within(
          Promise.allSettled(clients.map((client) => client.end())),
          5_000,
          "Clients did not close",
        ).catch(() => {});
        await proxy.close();
      }
    },
    EXHAUSTION_TEST_MS,
  );
});
