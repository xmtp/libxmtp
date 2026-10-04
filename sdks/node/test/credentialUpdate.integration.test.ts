import { createClient, createSigner } from "@test/helpers";
import { expect, it } from "vitest";

import { notificationBackend } from "./notificationBackend";

async function credentialUpdate(
  readThree: (read: () => Promise<unknown>) => Promise<void>,
): Promise<void> {
  const backend = await notificationBackend();
  let calls = 0;
  const client = await createClient(createSigner().signer, {
    backend: {
      url: backend.url,
      credentials: {
        credential: async () => {
          calls += 1;
          return {
            value: "Bearer callback-test",
            expiresAtSeconds: BigInt(Math.floor(Date.now() / 1000) + 3600),
          };
        },
      },
    },
  });
  const unknown = createSigner().identifier;
  const read = () => client.inboxIdFor(unknown);
  try {
    await readThree(read);
    expect(calls).toBe(1);
    await client.setCredential({
      value: "Bearer expired-test",
      expiresAtSeconds: 0n,
    });
    await readThree(read);
    expect(calls).toBe(2);
    await client.setCredential({
      value: "Bearer replacement-test",
      expiresAtSeconds: BigInt(Math.floor(Date.now() / 1000) + 3600),
    });
    const before = backend.requests.length;
    await read();
    expect(calls).toBe(2);
    expect(
      backend.requests
        .slice(before)
        .filter(({ path }) => path.endsWith("/GetInboxIds")),
    ).toEqual([
      {
        path: "/xmtp.backend.v1.IdentityService/GetInboxIds",
        authorization: "Bearer replacement-test",
      },
    ]);
  } finally {
    await client.end();
    await backend.close();
  }
}

it("deduplicates credential refresh and accepts expired and replacement client credentials", () =>
  credentialUpdate(async (read) => {
    await Promise.all([read(), read(), read()]);
  }));

it("accepts expired and replacement client credentials in sequence", () =>
  credentialUpdate(async (read) => {
    await read();
    await read();
    await read();
  }));
