import {
  Client,
  Timestamp,
  XmtpError,
  type ClientOptions,
  type ContentCodec,
  type Signer,
} from "@xmtp/react-native-sdk";

import { assert, equal } from "./assertions";
import { deferred, within } from "./async";

export async function boundaryJourney(
  client: Client,
  signer: Signer,
  options: ClientOptions,
): Promise<string[]> {
  const group = await client.conversations.createGroup([]);
  const negative = -9_007_199_254_740_993n;
  const retention = 9_007_199_254_740_993n;
  await group.updateDisappearingSettings({
    from: new Timestamp(negative),
    retentionNs: retention,
  });
  const restored = (await group.state()).common.disappearingSettings;
  assert(
    restored?.from instanceof Timestamp,
    "native nested timestamp must lift as Timestamp",
  );
  equal(
    restored.from.ns,
    negative,
    "native timestamp must retain signed full-width nanoseconds",
  );
  equal(
    restored.retentionNs,
    retention,
    "nested optional uint64 must retain its low bit",
  );
  await group.updateDisappearingSettings(undefined);
  equal(
    (await group.state()).common.isDisappearingEnabled,
    false,
    "absent update option must disable disappearing messages",
  );

  const codecFailure = new Error("RN constructor codec failure");
  let signerCalls = 0;
  const checkedSigner: Signer = {
    identity: async () => {
      signerCalls++;
      return signer.identity();
    },
    kind: async () => {
      signerCalls++;
      return signer.kind();
    },
    sign: async (request) => {
      signerCalls++;
      return signer.sign(request);
    },
  };
  const throwingCodec: ContentCodec<Uint8Array> = {
    get type(): ContentCodec<Uint8Array>["type"] {
      throw codecFailure;
    },
    encode: () => {
      throw codecFailure;
    },
    decode: () => {
      throw codecFailure;
    },
  };
  const codecOptions: ClientOptions = {
    ...options,
    storage: {
      location: {
        dbPath: `${client.storagePath!}.codec.db3`,
        attachmentsDir: `${client.storagePath!}.codec.attachments`,
      },
    },
    codecs: [throwingCodec],
  };
  let rejectedCodec: unknown;
  try {
    const unexpected = await Client.create(checkedSigner, codecOptions);
    await unexpected.end();
  } catch (error) {
    rejectedCodec = error;
  }
  equal(
    rejectedCodec,
    codecFailure,
    "constructor preserves the original throwing codec error",
  );
  equal(
    signerCalls,
    0,
    "throwing codec rejects before native allocation or signer callbacks",
  );
  const afterCodecFailure = await within(
    Client.create(signer, { ...codecOptions, codecs: [] }),
    "same database opens immediately after codec rejection",
  );
  try {
    assert(
      await afterCodecFailure.isRegistered(),
      "codec failure cannot retain native database ownership",
    );
  } finally {
    await afterCodecFailure.end();
  }

  const walletEntered = deferred<void>();
  const releaseWallet = deferred<void>();
  const secret = "RN_PRIVATE_WALLET_ERROR_174d";
  let signCalls = 0;
  const rejectedCreate = Client.create(
    {
      ...signer,
      async sign() {
        signCalls++;
        // A native callback must be able to await another call through the same JSI runtime.
        assert(
          await client.isRegistered(),
          "reentrant signer operation must finish",
        );
        walletEntered.resolve();
        await releaseWallet.promise;
        throw new Error(secret);
      },
    },
    { ...options, storage: { location: "inMemory" } },
  ).then(
    async (unexpected) => {
      await unexpected.end();
      return undefined;
    },
    (error: unknown) => error,
  );
  try {
    await within(walletEntered.promise, "wallet callback reenters native SDK");
    assert(
      await within(client.isRegistered(), "operation during held signer"),
      "held signer must not block unrelated client",
    );
  } finally {
    releaseWallet.resolve();
  }
  const walletFailure = await within(
    rejectedCreate,
    "rejected native signer completion",
  );
  assert(signCalls > 0, "wallet rejection must reach the signer callback");
  assert(
    walletFailure instanceof XmtpError.Signer,
    "wallet failure must keep the native Signer variant",
  );
  assert(
    !String(walletFailure).includes(secret),
    "native signer errors must redact callback content",
  );

  const credentialEntered = deferred<void>();
  const releaseCredential = deferred<void>();
  let credentialCalls = 0;
  const identity = await signer.identity();
  const failedAuth = Client.canMessage([identity], {
    url:
      typeof options.backend === "object" && "url" in options.backend
        ? options.backend.url
        : "",
    credentials: {
      async credential() {
        credentialCalls++;
        await client.canMessage([identity]);
        credentialEntered.resolve();
        await releaseCredential.promise;
        throw new Error("RN_PRIVATE_AUTH_RESPONSE_174d");
      },
    },
  }).then(
    () => undefined,
    (error: unknown) => error,
  );
  try {
    await within(
      credentialEntered.promise,
      "credential callback reenters native SDK",
    );
  } finally {
    releaseCredential.resolve();
  }
  const authFailure = await within(
    failedAuth,
    "rejected credential completion",
  );
  assert(
    credentialCalls > 0,
    "auth failure must reach the credential callback",
  );
  assert(
    authFailure instanceof XmtpError.CredentialCallbackFailed,
    "auth rejection must keep the structured CredentialCallbackFailed variant",
  );
  equal(
    authFailure.details.code,
    "CredentialCallbackFailed",
    "native error cause code",
  );
  equal(authFailure.details.category, "callback", "native error category");
  equal(authFailure.details.retryable, true, "native error retryability");
  assert(
    !String(authFailure).includes("RN_PRIVATE_AUTH_RESPONSE_174d"),
    "native auth failure must redact callback content",
  );
  return [
    "native signed timestamp and nested bigint/options; throwing codec before allocation and same-path recovery; held/reentrant/rejected wallet and auth callbacks with typed sanitized errors",
  ];
}

export async function storageJourney(
  client: Client,
  signer: Signer,
  options: ClientOptions,
): Promise<string[]> {
  assert(
    client.storagePath,
    "native storage journey needs the resolved private directory",
  );
  const dbPath = `${client.storagePath}.unkeyed.db3`;
  const unkeyedOptions: ClientOptions = {
    ...options,
    storage: {
      location: {
        dbPath,
        attachmentsDir: `${dbPath}.attachments`,
      },
    },
  };
  const first = await Client.create(signer, unkeyedOptions);
  const installation = first.installationId;
  const group = await first.conversations.createGroup([]);
  const id = await group.sendText("unkeyed reopen sentinel");
  await first.end();
  const reopened = await Client.create(signer, unkeyedOptions);
  try {
    equal(
      reopened.storagePath,
      dbPath,
      "explicit native path must remain exact",
    );
    equal(
      reopened.installationId,
      installation,
      "unkeyed reopen must retain installation",
    );
    equal(
      (await reopened.conversations.getMessageById(id))?.id,
      id,
      "unkeyed reopen must retain history",
    );
  } finally {
    await reopened.end();
  }

  const encryptedOptions: ClientOptions = {
    ...options,
    storage: {
      location: {
        dbPath: `${dbPath}.encrypted`,
        attachmentsDir: `${dbPath}.encrypted.attachments`,
      },
      encryptionKey: new Uint8Array(32).fill(93),
    },
  };
  const encrypted = await Client.create(signer, encryptedOptions);
  const encryptedInstallation = encrypted.installationId;
  await encrypted.end();
  let wrongKey: unknown;
  try {
    const unexpected = await Client.create(signer, {
      ...encryptedOptions,
      storage: {
        ...encryptedOptions.storage,
        encryptionKey: new Uint8Array(32).fill(94),
      },
    });
    await unexpected.end();
  } catch (error) {
    wrongKey = error;
  }
  assert(
    wrongKey instanceof XmtpError.Storage,
    "existing encrypted native database must reject a wrong key with Storage",
  );
  const correct = await Client.create(signer, encryptedOptions);
  try {
    equal(
      correct.installationId,
      encryptedInstallation,
      "wrong-key attempt must preserve encrypted database identity",
    );
  } finally {
    await correct.end();
  }
  await correct.storage.delete_();
  const recreated = await Client.create(signer, encryptedOptions);
  try {
    assert(
      recreated.installationId !== encryptedInstallation,
      "end then delete must remove native persistent installation",
    );
  } finally {
    await recreated.end();
  }
  return [
    "native explicit unkeyed reopen, typed wrong-key failure, correct-key recovery and end/delete/recreate",
  ];
}
