// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { generatePrivateKey } from "../../../../sdks/browser/node_modules/viem/_esm/accounts/generatePrivateKey.js";
// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { privateKeyToAccount } from "../../../../sdks/browser/node_modules/viem/_esm/accounts/privateKeyToAccount.js";
// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { toBytes } from "../../../../sdks/browser/node_modules/viem/_esm/utils/encoding/toBytes.js";
import { Client } from "../../../../target/sdk-generated/typescript-wasm/index";
import * as B from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk";

function check(condition: boolean, message: string): void {
  if (!condition) throw new Error(message);
}

// The package root creates and uses a Client without a session or a worker
// handle. A Message returns that same public Client.
export async function exercise(): Promise<void> {
  const account = privateKeyToAccount(generatePrivateKey());
  const identity = {
    identifier: account.address.toLowerCase(),
    kind: B.PublicIdentityKind.Ethereum,
  };
  const backend = B.BackendSource.Options.new({
    options: {
      url: `${location.origin}/backend`,
      appVersion: undefined,
      credential: undefined,
      credentials: undefined,
    },
  });
  const signer = (beforeSign?: () => Promise<void>) => ({
    async identity() {
      return identity;
    },
    async kind() {
      return B.SignerKind.Eoa.new();
    },
    async sign(request: { text: string }) {
      await beforeSign?.();
      const signed = await account.signMessage({ message: request.text });
      return B.Signature.Ecdsa.new(Uint8Array.from(toBytes(signed)).buffer);
    },
  });
  const options = {
    backend,
    storage: {
      location: B.StorageLocation.InMemory.new(),
      label: undefined,
      pool: undefined,
      singleConnection: false,
    },
    deviceSync: false,
    registration: { auto: true, nonce: undefined },
    forkRecovery: undefined,
    workers: undefined,
  };
  const client = await Client.create(signer(), options);
  check(await client.isRegistered(), "forwarded isRegistered failed");
  check(
    (await client.inboxState(false)).inboxId === client.inboxId(),
    "forwarded inboxState returned another inbox",
  );
  await client.catchUpToLive(undefined);
  check(
    (await Client.inboxIdFor(identity, backend)) === client.inboxId(),
    "static inboxIdFor did not run in the package worker",
  );
  const group = await client.conversations().createGroup([], undefined);
  const id = await group.sendText("public root", undefined);
  const message = (await group.messages(undefined)).find(
    (item) => item.id === id,
  );
  check(message !== undefined, "sent message missing");
  check(message!.client() === client, "Message did not return the app Client");
  check((await message!.refresh())?.id === id, "Message action failed");
  // A signer callback that ends its own client does not deadlock the end.
  const other = await Client.create(signer(), options);
  let ended: string | undefined;
  const revoke = client
    .revokeInstallations(
      signer(async () => {
        ended = await Promise.race([
          client.end().then(() => "ended"),
          new Promise<string>((resolve) =>
            setTimeout(() => resolve("end still pending"), 5000),
          ),
        ]);
      }),
      [other.installationId()],
    )
    .then(
      () => "revoked",
      () => "failed",
    );
  const settled = await Promise.race([
    revoke,
    new Promise<string>((resolve) =>
      setTimeout(() => resolve("revoke still pending"), 10000),
    ),
  ]);
  check(ended === "ended", `end from the signer callback: ${ended}`);
  check(settled !== "revoke still pending", "revoke did not settle");
  await other.end();
  await client.end();
  let closed = false;
  try {
    message!.client();
  } catch (error) {
    closed = B.XmtpError.ClientClosed.instanceOf(error);
  }
  check(closed, "Message kept its Client after end");
}
