import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { clientOptions, createSigner } from "@test/helpers";
import { Client, XmtpError, type ForkRecoveryOptions } from "@xmtp/node-sdk";
import { expect, it } from "vitest";

// verifies: IDENT-073, IDENT-074, IDENT-075, IDENT-076
it("waits for pre-authentication and stops signing after callback failure", async () => {
  const owner = createSigner().signer;
  const calls: string[] = [];
  const signer = {
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
  const client = await Client.create(
    signer,
    clientOptions({
      registration: { auto: false },
      handlers: { preAuthenticate: handler },
    }),
  );
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
    Client.create(
      signer,
      clientOptions({
        handlers: {
          preAuthenticate: {
            run: async () => {
              calls.push("preAuthenticate");
              throw new Error("refused");
            },
          },
        },
      }),
    ),
  ).rejects.toMatchObject({
    details: { code: "CallbackFailed", category: "callback", retryable: false },
  });
  expect(calls).toEqual(["preAuthenticate"]);
});

// verifies: FORK-070
it("preserves every fork recovery policy and response choice", async () => {
  const owner = createSigner().signer;
  let identityCalls = 0;
  const checkedSigner = {
    ...owner,
    identity: async () => {
      identityCalls++;
      return owner.identity();
    },
  };
  await expect(
    Client.create(
      checkedSigner,
      clientOptions({
        registration: { auto: false },
        forkRecovery: {
          policy: "allowlistedGroups",
          groups: ["invalid"],
          disableResponses: false,
        },
      }),
    ).then(async (unexpected) => {
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
      const client = await Client.create(
        createSigner().signer,
        clientOptions({ registration: { auto: false }, forkRecovery }),
      );
      try {
        expect(client.options.forkRecovery).toEqual(forkRecovery);
      } finally {
        await client.end();
      }
    }
  }
  const client = await Client.create(
    createSigner().signer,
    clientOptions({ registration: { auto: false } }),
  );
  try {
    expect(client.options.forkRecovery).toBeUndefined();
  } finally {
    await client.end();
  }
});

// verifies: STORE-007, STORE-009, STORE-011, STORE-017, STORE-018
it("rejects new builds and deletes file storage only after closing the client", async () => {
  const directory = mkdtempSync(join(tmpdir(), "xmtp-lifecycle-"));
  const path = join(directory, "client.db3");
  const options = clientOptions({
    storage: {
      location: {
        dbPath: path,
        attachmentsDir: join(directory, "attachments"),
      },
    },
  });
  const owner = createSigner();
  try {
    await expect(
      Client.build(owner.identifier, options),
    ).rejects.toBeInstanceOf(XmtpError.IdentityNotFound);
    expect(existsSync(path)).toBe(false);
    const client = await Client.create(owner.signer, options);
    try {
      expect(client.storagePath).toBe(path);
      expect(await client.storage.path()).toBe(path);
      await client.storage.delete_();
      expect(existsSync(path)).toBe(false);
      await expect(client.conversations.list()).rejects.toBeInstanceOf(
        XmtpError.ClientClosed,
      );
    } finally {
      await client.end();
    }
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
  const memory = await Client.create(
    createSigner().signer,
    clientOptions({ storage: { location: "inMemory" } }),
  );
  try {
    expect(memory.storagePath).toBeUndefined();
    expect(await memory.storage.path()).toBeUndefined();
    await expect(memory.storage.delete_()).rejects.toBeInstanceOf(
      XmtpError.InvalidInput,
    );
    expect(await memory.isRegistered()).toBe(true);
  } finally {
    await memory.end();
  }
});
