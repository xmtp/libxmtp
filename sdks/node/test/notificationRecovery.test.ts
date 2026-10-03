import { createRegisteredClient, createSigner } from "@test/helpers";
import { createRecoveryProxy } from "@test/recoveryProxy";
import { ConversationStream, XmtpError, type Client } from "@xmtp/node-sdk";
import { describe, expect, it, vi } from "vitest";

type NotificationStream = ConversationStream;

// Core has a ten-minute outage deadline. Use the production policy.
const OUTAGE_WAIT = 660_000;

describe("public notification recovery", () => {
  // verifies: PROC-038, PROC-039
  it.skipIf(process.env.XMTP_RECOVERY_BUDGET_TESTS !== "1")(
    "ends once on Core exhaustion and replaces from onError on the same offline client",
    async () => {
      const proxy = await createRecoveryProxy();
      const clients: Client[] = [];
      const streams: NotificationStream[] = [];
      let opening: Promise<NotificationStream> | undefined;
      let stopping = false;
      try {
        const sender = await createRegisteredClient(createSigner().signer, {
          deviceSync: false,
        });
        clients.push(sender);
        const receiver = await createRegisteredClient(createSigner().signer, {
          backend: { url: proxy.url },
          deviceSync: false,
        });
        clients.push(receiver);
        const replacementErrors: Error[] = [];
        const onError = vi.fn((_error: unknown) => {
          if (stopping) return;
          opening = (async () => {
            const replacement = ConversationStream.open(
              receiver,
              {},
              {
                onClose: (reason) => {
                  if (reason.kind === "failed")
                    replacementErrors.push(reason.error as Error);
                },
              },
            );
            streams.push(replacement);
            await replacement.ready();
            return replacement;
          })();
          void opening.catch(() => undefined);
        });
        const old = ConversationStream.open(
          receiver,
          {},
          {
            onClose: (reason) => {
              if (reason.kind === "failed") onError(reason.error);
            },
          },
        );
        await old.ready();
        streams.push(old);
        const initial = await sender.conversations.createGroup([
          receiver.inboxId,
        ]);
        expect((await old.next()).value?.id).toBe(initial.id);
        // Attach the rejection handler before the fault; preserve the actual cause.
        const failed = old.next().then(
          () => ({ error: undefined }),
          (error: unknown) => ({ error }),
        );
        proxy.disconnect();
        await expect
          .poll(() => onError.mock.calls.length, {
            timeout: OUTAGE_WAIT,
            interval: 100,
          })
          .toBe(1);
        const { error } = await failed;
        expect(error).toBeInstanceOf(Error);
        expect(error).toBeInstanceOf(XmtpError.RecoveryExhausted);
        expect(error).toMatchObject({
          details: { code: "RecoveryExhausted", category: "stream" },
        });
        expect(onError).toHaveBeenCalledWith(error);
        await expect(old.next()).rejects.toBe(error);
        expect(opening).toBeDefined();
        const replacement = await opening!;
        // Old cleanup cannot end the replacement that onError already opened.
        await old.end();
        const pending = replacement.next();
        void pending.catch(() => undefined);
        proxy.restore();
        const resumed = await sender.conversations.createGroup([
          receiver.inboxId,
        ]);
        expect((await pending).value?.id).toBe(resumed.id);
        expect(replacementErrors).toEqual([]);
        expect(onError).toHaveBeenCalledOnce();
      } finally {
        stopping = true;
        if (opening) await opening.catch(() => undefined);
        await Promise.allSettled(streams.map((stream) => stream.end()));
        await Promise.allSettled(clients.map((client) => client.end()));
        await proxy.close();
      }
    },
    OUTAGE_WAIT + 120_000,
  );
});
