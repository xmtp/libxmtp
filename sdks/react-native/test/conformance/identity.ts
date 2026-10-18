import {
  Client,
  LocalSigningKey,
  type ClientOptions,
} from "@xmtp/react-native-sdk";

import { assert, bytesEqual, equal } from "./assertions";
import { within } from "./async";

/** Bounded real native-object, account, handle, installation and sync controls. */
export async function identityJourney(
  options: ClientOptions,
): Promise<string[]> {
  assert(options.backend, "identity journey requires explicit backend");
  const fixed = "0x" + "0".repeat(63) + "1";
  const known = await LocalSigningKey.fromHex(fixed);
  equal(
    (await known.identity()).identifier,
    "0x7e5f4552091a69125d5dfcb7b8c2659029395bdf",
    "native signing key derives known identity",
  );
  const bytes = await known.toBytes();
  bytesEqual(
    bytes,
    Array.from({ length: 32 }, (_, index) => (index === 31 ? 1 : 0)),
    "native signing-key export retains all 32 exact bytes",
  );
  const backing = new Uint8Array(34);
  backing.set(bytes, 1);
  const sliced = await LocalSigningKey.fromBytes(backing.subarray(1, 33));
  bytes.fill(0);
  equal(await known.toHex(), fixed, "native key exports independent bytes");
  equal(await sliced.toHex(), fixed, "native key reads sliced private bytes");
  const signature = await known.sign({ text: "RN native key roundtrip" });
  const restoredSignature = await sliced.sign({
    text: "RN native key roundtrip",
  });
  assert(
    signature.kind === "ecdsa" && restoredSignature.kind === "ecdsa",
    "native key returns Ethereum signature variant",
  );
  bytesEqual(
    signature.value,
    Array.from(restoredSignature.value),
    "native key signature bytes roundtrip",
  );

  const key = await Client.generateKeys();
  const restored = await LocalSigningKey.fromHex(await key.toHex());
  const nativeOptions: ClientOptions = {
    ...options,
    deviceSync: true,
    storage: { location: "inMemory" },
  };
  const owner = await within(
    Client.create(restored, nativeOptions),
    "actual native signing key creates client",
  );
  let second: Client | undefined;
  try {
    assert(
      await owner.isRegistered(),
      "native LocalSigningKey registers client directly",
    );
    const identity = await key.identity();
    equal(
      owner.identity.identifier,
      identity.identifier,
      "native key object retains identity through client creation",
    );
    const linked = await Client.generateKeys();
    const linkedIdentity = await linked.identity();
    await owner.unsafeAddAccount(linked, false);
    assert(
      (await owner.inboxState(true)).identities.some(
        (item) => item.identifier === linkedIdentity.identifier,
      ),
      "native account addition appears in association state",
    );
    await owner.removeAccount(restored, linkedIdentity);
    assert(
      !(await owner.inboxState(true)).identities.some(
        (item) => item.identifier === linkedIdentity.identifier,
      ),
      "native account removal updates association state",
    );

    const handle = `rn-${owner.inboxId.slice(0, 24)}`;
    const available = await owner.availableHandles(handle);
    assert(
      available.includes(handle),
      "native handle availability includes fresh handle",
    );
    await owner.claimHandle(handle);
    equal(
      (await owner.inboxState(true)).handle,
      handle,
      "native claimed handle appears in inbox state",
    );
    equal(
      await Client.inboxIdFor(
        { kind: "handle", identifier: handle },
        options.backend,
      ),
      owner.inboxId,
      "native static handle lookup resolves claimed inbox",
    );

    second = await Client.create(restored, nativeOptions);
    assert(
      second.installationId !== owner.installationId,
      "second native client has independent installation",
    );
    assert(
      await Client.isInstallationAuthorized(
        owner.inboxId,
        second.installationId,
        options.backend,
      ),
      "native installation authorization sees second client",
    );
    const signed = await second.signWithInstallationKey(
      "RN installation bytes",
    );
    assert(
      await Client.verifySignedWithPublicKey(
        "RN installation bytes",
        signed,
        second.installationIdBytes,
      ),
      "native installation signature verifies through static bytes boundary",
    );
    let summary = await owner.syncAllDeviceSyncGroups();
    const syncDeadline = Date.now() + 15_000;
    while (summary.eligible === 0n && Date.now() < syncDeadline) {
      await new Promise<void>((resolve) => setTimeout(resolve, 50));
      summary = await owner.syncAllDeviceSyncGroups();
    }
    equal(
      typeof summary.eligible,
      "bigint",
      "native sync eligible count is bigint",
    );
    equal(
      typeof summary.synced,
      "bigint",
      "native sync completed count is bigint",
    );
    assert(
      summary.eligible > 0n && summary.synced === summary.eligible,
      "enabled device sync owns and synchronizes a native sync group",
    );
    await owner.revokeInstallations(restored, [second.installationId]);
    assert(
      !(await owner.inboxState(true)).installations.some(
        (item) => item.id === second!.installationId,
      ),
      "native installation revocation removes second client",
    );
    const own = await owner.ownInboxUpdatesCount(true);
    equal(
      (await owner.latestInboxUpdatesCount([owner.inboxId], true)).get(
        owner.inboxId,
      ),
      own,
      "native identity map retains bigint count",
    );
    return [
      "positive native LocalSigningKey slice/export/sign/create roundtrip; account add/remove, handle claim/lookup, installation signature/revoke and enabled device-sync group",
    ];
  } finally {
    await second?.end();
    await owner.end();
  }
}
