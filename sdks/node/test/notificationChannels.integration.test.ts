import { createClient, createSigner, clientOptions } from "@test/helpers";
import { XmtpError } from "@xmtp/node-sdk";
import { describe, expect, it } from "vitest";

import { notificationBackend } from "./notificationBackend";

describe("native notification request fields", () => {
  it.each(["apns", "fcm"] as const)(
    "registers the exact %s channel and token and restores its state",
    async (kind) => {
      const backend = await notificationBackend();
      const signer = createSigner().signer;
      const options = clientOptions({ backend: { url: backend.url } });
      let client: Awaited<ReturnType<typeof createClient>> | undefined;
      try {
        client = await createClient(signer, options);
        const token = `${kind}-exact-token`;
        expect(
          await client.enableNotifications({
            channel: { kind, token },
            consentStates: [],
            includeWelcomes: false,
            includeSyncGroups: false,
            includeCommits: true,
          }),
        ).toEqual({ kind: "enabled" });
        expect(backend.registrations).toHaveLength(1);
        expect(backend.registrations[0]?.[kind]).toEqual({ token });
        expect(
          backend.registrations[0]?.[kind === "apns" ? "fcm" : "apns"],
        ).toBeUndefined();
        expect(backend.registrations[0]?.http).toBeUndefined();
        expect(backend.registrations[0]?.includeWelcomes ?? false).toBe(false);
        expect(backend.registrations[0]?.includeSyncGroups ?? false).toBe(false);
        expect(backend.registrations[0]?.includeCommits).toBe(true);
        expect(backend.registrations[0]?.consentStates ?? []).toEqual([]);
        await client.end();
        client = await createClient(signer, options);
        expect(client.notificationState()).toEqual({ kind: "enabled" });
      } finally {
        await client?.end();
        await backend.close();
      }
    },
  );

  it.each([15, 16, 64, 65])(
    "validates a %i-byte HTTP key before registration",
    async (size) => {
      const backend = await notificationBackend();
      let client: Awaited<ReturnType<typeof createClient>> | undefined;
      try {
        client = await createClient(createSigner().signer, {
          backend: { url: backend.url },
        });
        const signingKey = new Uint8Array(size).fill(7);
        const configuration = {
          channel: {
            kind: "http" as const,
            url: "https://example.test",
            signingKey,
          },
          includeWelcomes: false,
        };
        if (size === 16 || size === 64) {
          expect(await client.enableNotifications(configuration)).toEqual({
            kind: "enabled",
          });
          expect(backend.registrations).toHaveLength(1);
          expect(backend.registrations[0]?.http).toEqual({
            url: configuration.channel.url,
            signingKey: [...signingKey],
          });
          expect(backend.registrations[0]?.apns).toBeUndefined();
          expect(backend.registrations[0]?.fcm).toBeUndefined();
        } else {
          await expect(
            client.enableNotifications(configuration),
          ).rejects.toBeInstanceOf(XmtpError.InvalidArgument);
          expect(backend.registrations).toHaveLength(0);
          expect(client.notificationState()).toEqual({ kind: "disabled" });
        }
      } finally {
        await client?.end();
        await backend.close();
      }
    },
  );

  it.each(["apns", "fcm"] as const)(
    "rejects an empty %s token before registration",
    async (kind) => {
      const backend = await notificationBackend();
      let client: Awaited<ReturnType<typeof createClient>> | undefined;
      try {
        client = await createClient(createSigner().signer, {
          backend: { url: backend.url },
        });
        await expect(
          client.enableNotifications({ channel: { kind, token: "" } }),
        ).rejects.toBeInstanceOf(XmtpError.InvalidArgument);
        expect(backend.registrations).toHaveLength(0);
        expect(client.notificationState()).toEqual({ kind: "disabled" });
      } finally {
        await client?.end();
        await backend.close();
      }
    },
  );
});
