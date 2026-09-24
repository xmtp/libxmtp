import { realpathSync } from "node:fs";
import { mkdtemp } from "node:fs/promises";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { performance } from "node:perf_hooks";
import { fileURLToPath, pathToFileURL } from "node:url";

import * as sdk from "../../../../target/sdk-generated/typescript-napi/index.ts";

const require = createRequire(import.meta.url);
const native = require(process.env.SDK_NAPI_BIN!);
const viemRoot = realpathSync(
  fileURLToPath(
    new URL("../../../../sdks/node/node_modules/viem", import.meta.url),
  ),
);
const { generatePrivateKey, privateKeyToAccount } = await import(
  pathToFileURL(join(viemRoot, "_esm/accounts/index.js")).href
);
const { toBytes } = await import(
  pathToFileURL(join(viemRoot, "_esm/index.js")).href
);

async function median20(action: () => Promise<unknown>): Promise<number> {
  const samples: number[] = [];
  for (let i = 0; i < 20; i++) {
    const start = performance.now();
    await action();
    samples.push(performance.now() - start);
  }
  samples.sort((a, b) => a - b);
  return (samples[9]! + samples[10]!) / 2;
}

await sdk.uniffiInitAsync();
const account = privateKeyToAccount(generatePrivateKey());
const identity = {
  identifier: account.address.toLowerCase(),
  kind: sdk.PublicIdentityKind.Ethereum,
};
const signer = {
  async identity() {
    return identity;
  },
  async kind() {
    return new sdk.SignerKind.Eoa();
  },
  async sign(request: { text: string }) {
    return new sdk.Signature.Ecdsa(
      Uint8Array.from(
        toBytes(await account.signMessage({ message: request.text })),
      ).buffer,
    );
  },
};
const directory = await mkdtemp(join(tmpdir(), "xmtp-sdk-bench-"));
const url = process.env.XMTP_BACKEND_URL ?? "http://127.0.0.1:9150";
const options = {
  backend: { url, appVersion: undefined, credentials: undefined },
  storage: {
    location: new sdk.StorageLocation.Directory(directory),
    label: undefined,
    encryptionKey: undefined,
  },
  deviceSync: false,
};
const client = await sdk.Client.create(signer, options);
const group = await client.conversations().createGroup([]);
const groupID = group.id().toString();
const inboxID = client.inboxID().toString();
const dbPath = join(directory, `xmtp-${inboxID}.db3`);
const nativeClient = await native.createClient(
  url,
  { dbPath },
  inboxID,
  {
    identifier: identity.identifier,
    identifierKind: native.IdentifierKind.Ethereum,
  },
  native.SyncWorkerMode.Disabled,
  undefined,
  { level: native.LogLevel.Error },
);
const nativeGroup = nativeClient.conversations().getConversationById(groupID);

const sdkEmptyCount = (await group.messages()).length;
const nativeEmptyCount = (await nativeGroup.listMessages()).length;
if (sdkEmptyCount !== 0 || nativeEmptyCount !== 0) {
  throw new Error(
    `expected zero-row page; SDK=${sdkEmptyCount}, NAPI=${nativeEmptyCount}`,
  );
}
const sdkEmpty = await median20(() => group.messages());
const nativeEmpty = await median20(() => nativeGroup.listMessages());

for (let i = 0; i < 10_000; i++) {
  await nativeGroup.sendText(`message ${i}`, {
    optimistic: true,
    idempotencyKey: `bench-${i}`,
  });
  if ((i + 1) % 1000 === 0) console.log(`Seeded ${i + 1}/10000 messages`);
}
const sdkCount = (await group.messages()).length;
const nativeCount = (await nativeGroup.listMessages()).length;
if (sdkCount !== 10_000 || nativeCount !== 10_000) {
  throw new Error(
    `expected 10,000-message page; SDK=${sdkCount}, NAPI=${nativeCount}`,
  );
}
const sdkPage = await median20(() => group.messages());
const nativePage = await median20(() => nativeGroup.listMessages());

console.log(
  `Machine: ${process.platform}-${process.arch}, Node ${process.version}`,
);
console.log("Median of 20 runs (ms):");
console.log("| Call | @ubjs/node | bindings/node | ratio |");
console.log("| --- | ---: | ---: | ---: |");
console.log(
  `| Empty async page | ${sdkEmpty.toFixed(3)} | ${nativeEmpty.toFixed(3)} | ${(sdkEmpty / nativeEmpty).toFixed(2)}x |`,
);
console.log(
  `| 10,000-message page | ${sdkPage.toFixed(3)} | ${nativePage.toFixed(3)} | ${(sdkPage / nativePage).toFixed(2)}x |`,
);
await nativeClient.close();
await client.end();
if (sdkPage > 2 * nativePage) process.exitCode = 2;
