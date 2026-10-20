import {
  Client,
  Timestamp,
  XmtpError,
  type ClientOptions,
  type Signer,
} from "@xmtp/react-native-sdk";

import { assert, equal } from "./assertions";
import { deferred, within } from "./async";

/** Values and callbacks that only the JSI boundary can break; Rust owns the rest. */
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
  return [
    "native signed timestamp and nested bigint/options; held, re-entrant and rejected wallet callback with a typed, sanitized error",
  ];
}
