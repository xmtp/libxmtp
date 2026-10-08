import { createClient, createSigner } from "@test/helpers";
import { expect, it } from "vitest";

import { notificationBackend } from "./notificationBackend";

it("deduplicates concurrent and sequential credential reads across expiry and replacement", async () => {
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
  const expired = { value: "Bearer expired-test", expiresAtSeconds: 0n };
  try {
    // Concurrent reads share the first missing credential.
    await Promise.all([read(), read(), read()]);
    expect(calls).toBe(1);

    // The first sequential read must refresh an expired credential itself.
    await client.setCredential(expired);
    await read();
    expect(calls).toBe(2);
    await read();
    await read();
    expect(calls).toBe(2);

    // Concurrent reads also share a refresh after the credential expires.
    await client.setCredential(expired);
    await Promise.all([read(), read(), read()]);
    expect(calls).toBe(3);

    // Repeat the single-read miss after another explicit expiry.
    await client.setCredential(expired);
    await read();
    expect(calls).toBe(4);
    await read();
    await read();
    expect(calls).toBe(4);

    await client.setCredential({
      value: "Bearer replacement-test",
      expiresAtSeconds: BigInt(Math.floor(Date.now() / 1000) + 3600),
    });
    const before = backend.requests.length;
    await read();
    expect(calls).toBe(4);
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
});
