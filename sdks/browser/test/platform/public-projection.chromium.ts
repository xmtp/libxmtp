import { generatePrivateKey } from "../../../../sdks/browser/node_modules/viem/_esm/accounts/generatePrivateKey.js";
import { privateKeyToAccount } from "../../../../sdks/browser/node_modules/viem/_esm/accounts/privateKeyToAccount.js";
import { hexToBytes } from "../../../../sdks/browser/node_modules/viem/_esm/utils/encoding/toBytes.js";
import { createInWorker } from "../../../../target/sdk-generated/typescript-wasm/package-session.gen";
import {
  Client,
  Storage,
} from "../../../../target/sdk-generated/typescript-wasm/proxy.gen";
import * as Public from "../../../../target/sdk-generated/typescript-wasm/public-values.gen";

const projection = new Proxy({} as Public.ObjectProjection, {
  get(_target, name) {
    if (name === "isBackend") return () => false;
    return () => {
      throw new Error(`unexpected object ${String(name)}`);
    };
  },
});

export async function exercise(path: string): Promise<void> {
  const account = privateKeyToAccount(generatePrivateKey());
  let signs = 0;
  const identity: Public.PublicIdentity = {
    kind: "ethereum",
    identifier: account.address.toLowerCase(),
  };
  const signer: Public.Signer = {
    async identity() {
      return identity;
    },
    async kind() {
      return { kind: "eoa" };
    },
    async sign(request) {
      signs++;
      const bytes = hexToBytes(
        await account.signMessage({ message: request.text }),
      );
      const padded = new Uint8Array(bytes.length + 2);
      padded.set(bytes, 1);
      return { kind: "ecdsa", value: padded.subarray(1, -1) };
    },
  };
  const options: Public.ClientOptions = {
    backend: { url: `${location.origin}/backend` },
    storage: {
      location: { dbPath: path, attachmentsDir: `${path}-attachments` },
      label: path,
      singleConnection: false,
    },
    deviceSync: false,
    allowOffline: false,
    registration: { auto: true },
  };
  const client = await createInWorker((session) =>
    Client.create(
      session,
      Public.lowerSigner(signer, projection),
      Public.lowerClientOptions(options, projection),
    ),
  );
  const admin = await Storage.admin();
  try {
    if (signs === 0) throw new Error("projected signer was not called");
    if (!(await client.isRegistered()))
      throw new Error("projected registration failed");
    const received = Public.liftPublicIdentity(client.identity(), projection);
    if (
      received.kind !== "ethereum" ||
      received.identifier !== identity.identifier
    )
      throw new Error("projected identity changed");
    if ((await client.storage().path()) !== path)
      throw new Error("projected storage location changed");
  } finally {
    await client.end();
    await admin.deleteFile(path);
    await admin.end();
  }
}
