import { Client, type ForkRecoveryOptions } from "@xmtp/browser-sdk";
import { expect, it } from "vitest";

import { options, signer } from "./helpers";

// verifies: IDENT-073, IDENT-074, IDENT-075, IDENT-076
it("waits for pre-authentication and stops signing after callback failure", async () => {
  const owner = signer();
  const calls: string[] = [];
  const recordingSigner = {
    ...owner,
    sign: async (request: Parameters<typeof owner.sign>[0]) => {
      calls.push("sign");
      return owner.sign(request);
    },
  };
  let release!: () => void;
  let entered!: () => void;
  const started = new Promise<void>((resolve) => {
    entered = resolve;
  });
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  const handler = {
    run: async () => {
      calls.push("preAuthenticate");
      entered();
      await held;
    },
  };
  const client = await Client.create(recordingSigner, {
    ...options,
    registration: { auto: false },
    handlers: { preAuthenticate: handler },
  });
  try {
    expect(calls).toEqual([]);
    const registration = client.register();
    await started;
    expect(calls).toEqual(["preAuthenticate"]);
    release();
    await registration;
    expect(calls).toEqual(["preAuthenticate", "sign"]);
    calls.length = 0;
    await client.register();
    expect(calls).toEqual([]);
  } finally {
    release();
    await client.end();
  }
  await expect(
    Client.create(recordingSigner, {
      ...options,
      handlers: {
        preAuthenticate: {
          run: async () => {
            calls.push("preAuthenticate");
            throw new Error("refused");
          },
        },
      },
    }),
  ).rejects.toMatchObject({
    details: { code: "CallbackFailed", category: "callback", retryable: false },
  });
  expect(calls).toEqual(["preAuthenticate"]);
});

// verifies: FORK-070
it("preserves every fork recovery policy and response choice", async () => {
  for (const policy of ["none", "allowlistedGroups", "all"] as const) {
    for (const disableResponses of [false, true]) {
      const forkRecovery: ForkRecoveryOptions = {
        policy,
        groups: ["13".repeat(16)],
        disableResponses,
      };
      const client = await Client.create(signer(), {
        ...options,
        registration: { auto: false },
        forkRecovery,
      });
      try {
        expect(client.options.forkRecovery).toEqual(forkRecovery);
      } finally {
        await client.end();
      }
    }
  }
  const client = await Client.create(signer(), {
    ...options,
    registration: { auto: false },
  });
  try {
    expect(client.options.forkRecovery).toBeUndefined();
  } finally {
    await client.end();
  }
});
