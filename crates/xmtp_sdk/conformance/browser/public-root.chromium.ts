// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { generatePrivateKey } from "../../../../sdks/browser/node_modules/viem/_esm/accounts/generatePrivateKey.js";
// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { privateKeyToAccount } from "../../../../sdks/browser/node_modules/viem/_esm/accounts/privateKeyToAccount.js";
// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { toBytes } from "../../../../sdks/browser/node_modules/viem/_esm/utils/encoding/toBytes.js";
import * as sdk from "../../../../target/sdk-generated/typescript-wasm/index";

function check(condition: boolean, message: string): void {
  if (!condition) throw new Error(message);
}

// The package root creates and uses a Client without a session or a worker
// handle. A Message returns that same public Client.
export async function exercise(): Promise<void> {
  const account = privateKeyToAccount(generatePrivateKey());
  const identity: sdk.PublicIdentity = {
    identifier: account.address.toLowerCase(),
    kind: "ethereum",
  };
  const backend: sdk.BackendOptions = { url: `${location.origin}/backend` };
  const signer = (beforeSign?: () => Promise<void>): sdk.Signer => ({
    async identity() {
      return identity;
    },
    async kind() {
      return { kind: "eoa" };
    },
    async sign(request) {
      await beforeSign?.();
      const signed = await account.signMessage({ message: request.text });
      return { kind: "ecdsa", value: Uint8Array.from(toBytes(signed)) };
    },
  });
  const options: sdk.ClientOptions = {
    backend,
    storage: { location: "inMemory" },
    deviceSync: false,
  };
  const client = await sdk.Client.create(signer(), options);
  check(await client.isRegistered(), "forwarded isRegistered failed");
  check(
    (await client.inboxState(false)).inboxId === client.inboxId,
    "forwarded inboxState returned another inbox",
  );
  await client.catchUpToLive(undefined);
  check(
    (await sdk.Client.inboxIdFor(identity, backend)) === client.inboxId,
    "static inboxIdFor did not run in the package worker",
  );
  const group = await client.conversations.createGroup([]);
  const id = await group.sendText("public root");
  const message = (await group.messages()).find((item) => item.id === id);
  check(message !== undefined, "sent message missing");
  check(message!.client() === client, "Message did not return the app Client");
  check((await message!.refresh())?.id === id, "Message action failed");
  // A signer callback that ends its own client does not deadlock the end.
  const other = await sdk.Client.create(signer(), options);
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
      [other.installationId],
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
    closed = error instanceof sdk.XmtpError.ClientClosed;
  }
  check(closed, "Message kept its Client after end");
}
