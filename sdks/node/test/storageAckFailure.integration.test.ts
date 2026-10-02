import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";

import { createRegisteredClient, createSigner } from "@test/helpers";
import {
  MessageStream,
  XmtpError,
  type Client,
  type StreamCloseReason,
} from "@xmtp/node-sdk";
import { expect, it } from "vitest";

const ACK_FAILURE = "xmtp_test_ack_failure";

async function within<T>(operation: Promise<T>, label: string): Promise<T> {
  let timer: ReturnType<typeof setTimeout>;
  try {
    return await Promise.race([
      operation,
      new Promise<never>((_, reject) => {
        timer = setTimeout(() => reject(new Error(label)), 20_000);
      }),
    ]);
  } finally {
    clearTimeout(timer!);
  }
}

// verifies: PROC-028, PROC-040, PROC-041
it.each(["group", "all"] as const)(
  "ends %s callbacks on a real SQLite ACK failure and replays without a later handoff",
  async (scope) => {
    const directory = await mkdtemp(join(tmpdir(), "xmtp-node-ack-failure-"));
    const dbPath = join(directory, "client.db3");
    const streams: MessageStream[] = [];
    let client: Client | undefined;
    let database: DatabaseSync | undefined;
    let consumption: Promise<unknown> | undefined;
    const closeReasons: StreamCloseReason[] = [];
    const received: string[] = [];
    let faultInstalled = false;
    try {
      client = await createRegisteredClient(createSigner().signer, {
        storage: {
          location: {
            dbPath,
            attachmentsDir: join(directory, "attachments"),
          },
        },
      });
      const group = await client.conversations.createGroup([]);
      const firstId = await group.sendText("fail this acknowledgement");
      const secondId = await group.sendText("must remain queued");
      const history = await group.messageHistorySnapshot(128);
      const firstPosition = history.messages.findIndex(
        (message) => message.id === firstId,
      );
      expect(firstPosition).toBeGreaterThanOrEqual(0);
      const expectedPrefix = history.messages
        .slice(0, firstPosition + 1)
        .map((message) => message.id);
      const firstCursor = history.messages[firstPosition]?.deliveryCursor;
      expect(typeof firstCursor).toBe("string");

      // This connection changes only this test's unencrypted database.
      // The trigger runs inside the native SDK's real ACK transaction.
      database = new DatabaseSync(dbPath);
      database.exec("PRAGMA busy_timeout = 5000");
      const open = (onClose?: (reason: StreamCloseReason) => void) => {
        const stream =
          scope === "group"
            ? MessageStream.openGroup(client!, group, undefined, { onClose })
            : MessageStream.open(
                client!,
                { conversationKind: "group" },
                { onClose },
              );
        streams.push(stream);
        return stream;
      };
      const failed = open((reason) => closeReasons.push(reason));
      await failed.ready();
      consumption = failed.onValue((message) => {
        received.push(message.id);
        if (message.id !== firstId) return;
        expect(faultInstalled).toBe(false);
        // Delivery progress uses entity_kind 10. BEFORE INSERT also covers
        // the upsert used to acknowledge after an earlier history item.
        database!.exec(`
          CREATE TRIGGER xmtp_test_ack_failure
          BEFORE INSERT ON refresh_state
          WHEN NEW.entity_kind = 10
          BEGIN SELECT RAISE(ABORT, '${ACK_FAILURE}'); END;
        `);
        faultInstalled = true;
      });
      const outcome = await within(
        consumption.then(
          () => ({ kind: "completed" as const }),
          (error: unknown) => ({ kind: "failed" as const, error }),
        ),
        "ACK failure did not end callback consumption",
      );
      expect(faultInstalled).toBe(true);
      expect(received).toEqual(expectedPrefix);
      expect(received).not.toContain(secondId);
      expect(outcome.kind).toBe("failed");
      if (outcome.kind !== "failed")
        throw new Error("Callback consumption lost its storage error");
      expect(outcome.error).toBeInstanceOf(XmtpError.Storage);
      if (!(outcome.error instanceof XmtpError.Storage))
        throw new Error("Expected the typed native storage error");
      expect(outcome.error.details.category).toBe("storage");
      expect(outcome.error.details.message).toContain(ACK_FAILURE);
      expect(closeReasons).toHaveLength(1);
      expect(closeReasons[0]?.kind).toBe("failed");
      if (closeReasons[0]?.kind !== "failed")
        throw new Error("Expected one failed close notification");
      expect(closeReasons[0].error).toBe(outcome.error);
      await expect(failed.next()).rejects.toBe(outcome.error);

      database.exec("DROP TRIGGER xmtp_test_ack_failure");
      faultInstalled = false;
      const replay = open();
      await replay.ready();
      // Ending the failed stream again must not close its replacement.
      await failed.end();
      const first = await within(replay.next(), "first replay hung");
      expect(first.value?.id).toBe(firstId);
      expect(first.value?.deliveryCursor).toBe(firstCursor);
      expect(
        (await within(replay.next(), "second replay hung")).value?.id,
      ).toBe(secondId);
      expect(received).toEqual(expectedPrefix);
      expect(closeReasons).toHaveLength(1);
      await expect(failed.next()).rejects.toBe(outcome.error);
    } finally {
      if (database && faultInstalled)
        database.exec("DROP TRIGGER IF EXISTS xmtp_test_ack_failure");
      await Promise.allSettled(
        streams.map((stream) => within(stream.end(), "stream cleanup hung")),
      );
      if (consumption)
        await within(consumption, "callback cleanup hung").catch(() => {});
      database?.close();
      if (client) await within(client.end(), "client cleanup hung");
      await rm(directory, { recursive: true, force: true });
    }
  },
);
