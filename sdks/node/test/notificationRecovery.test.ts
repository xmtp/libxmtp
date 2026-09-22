import { createRegisteredClient, createSigner } from "@test/helpers";
import { createRecoveryProxy } from "@test/recoveryProxy";
import { describe, expect, it, vi } from "vitest";

import type { Client } from "@/Client";

type NotificationStream = Awaited<
  ReturnType<Client["conversations"]["stream"]>
>;

// Core has a ten-minute outage deadline. Use the production policy.
const OUTAGE_WAIT = 660_000;

describe("public notification recovery", () => {
  // verifies: PROC-038, PROC-039
  it(
    "ends once on Core exhaustion and replaces from onError on the same offline client",
    async () => {
      const proxy = await createRecoveryProxy();
      const clients: Client[] = [];
      const streams: NotificationStream[] = [];
      let opening: Promise<NotificationStream> | undefined;
      let stopping = false;
      try {
        const sender = await createRegisteredClient(createSigner().signer, {
          disableDeviceSync: true,
        });
        clients.push(sender);
        const receiver = await createRegisteredClient(createSigner().signer, {
          backendUrl: proxy.url,
          disableDeviceSync: true,
        });
        clients.push(receiver);
        const replacementErrors: Error[] = [];
        const onRetry = vi.fn();
        const onError = vi.fn(() => {
          if (stopping) return;
          opening = receiver.conversations
            .stream({
              disableSync: true,
              onError: (error) => replacementErrors.push(error),
              onRetry,
            })
            .then((stream) => {
              streams.push(stream);
              return stream;
            });
          void opening.catch(() => undefined);
        });
        const old = await receiver.conversations.stream({
          disableSync: true,
          onError,
          onRetry,
        });
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
        expect((error as Error).message).toContain(
          "[LocalDeliveryError::NetworkRecoveryExhausted]",
        );
        expect(old.isDone).toBe(true);
        expect(onRetry).not.toHaveBeenCalled();
        expect(opening).toBeDefined();
        const replacement = await opening!;
        expect(replacement.isDone).toBe(false);
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
        expect(onRetry).not.toHaveBeenCalled();
      } finally {
        stopping = true;
        if (opening) await opening.catch(() => undefined);
        await Promise.allSettled(streams.map((stream) => stream.end()));
        await Promise.allSettled(clients.map((client) => client.close()));
        await proxy.close();
      }
    },
    OUTAGE_WAIT + 120_000,
  );
});
