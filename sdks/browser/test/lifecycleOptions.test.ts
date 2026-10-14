import {
  Client,
  Storage,
  XmtpError,
  type ForkRecoveryOptions,
} from "@xmtp/browser-sdk";
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
    const status = await Promise.race([
      registration.then(
        () => "settled",
        () => "settled",
      ),
      new Promise<string>((resolve) => setTimeout(() => resolve("held"), 100)),
    ]);
    expect(status).toBe("held");
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
  const owner = signer();
  let identityCalls = 0;
  const checkedSigner = {
    ...owner,
    identity: async () => {
      identityCalls++;
      return owner.identity();
    },
  };
  await expect(
    Client.create(checkedSigner, {
      ...options,
      registration: { auto: false },
      forkRecovery: {
        policy: "allowlistedGroups",
        groups: ["invalid"],
        disableResponses: false,
      },
    }).then(async (unexpected) => {
      await unexpected.end();
      throw new Error("Invalid fork group reached client creation");
    }),
  ).rejects.toBeInstanceOf(XmtpError.InvalidArgument);
  expect(identityCalls).toBe(0);

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

// verifies: STORE-007
it("public build rejects an absent identity without creating a database", async () => {
  const path = "lifecycle-build-" + crypto.randomUUID() + ".db3";
  const admin = await Storage.admin();
  try {
    expect(await admin.fileExists(path)).toBe(false);
    await expect(
      Client.build(await signer().identity(), {
        ...options,
        storage: {
          location: { dbPath: path, attachmentsDir: path + ".attachments" },
        },
      }).then(async (unexpected) => {
        await unexpected.end();
        throw new Error("A build opened a database with no identity");
      }),
    ).rejects.toBeInstanceOf(XmtpError.IdentityNotFound);
    expect(await admin.fileExists(path)).toBe(false);
  } finally {
    if (await admin.fileExists(path)) await admin.deleteFile(path);
    await admin.end();
  }
});
