import { existsSync, realpathSync } from "node:fs";
import { copyFile, mkdtemp } from "node:fs/promises";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { performance } from "node:perf_hooks";
import { fileURLToPath, pathToFileURL } from "node:url";

import * as sdk from "../../../../target/sdk-bench/typescript-napi/index.ts";

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

function median(samples: number[]): number {
  samples.sort((a, b) => a - b);
  return (samples[9]! + samples[10]!) / 2;
}

async function median20(action: () => Promise<unknown>): Promise<number> {
  const samples: number[] = [];
  for (let i = 0; i < 20; i++) await action();
  for (let i = 0; i < 20; i++) {
    const start = performance.now();
    await action();
    samples.push(performance.now() - start);
  }
  return median(samples);
}

async function compare20(
  sdkCall: () => Promise<unknown>,
  nativeCall: () => Promise<unknown>,
): Promise<[number, number]> {
  const sdkSamples: number[] = [];
  const nativeSamples: number[] = [];
  for (let i = 0; i < 20; i++) {
    await sdkCall();
    await nativeCall();
  }
  for (let i = 0; i < 20; i++) {
    const measure = async (call: () => Promise<unknown>, samples: number[]) => {
      const start = performance.now();
      await call();
      samples.push(performance.now() - start);
    };
    if (i % 2 === 0) {
      await measure(sdkCall, sdkSamples);
      await measure(nativeCall, nativeSamples);
    } else {
      await measure(nativeCall, nativeSamples);
      await measure(sdkCall, sdkSamples);
    }
  }
  return [median(sdkSamples), median(nativeSamples)];
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
const seedDirectory = await mkdtemp(join(tmpdir(), "xmtp-sdk-bench-seed-"));
const url = process.env.XMTP_BACKEND_URL!;
const options = (directory: string) => ({
  backend: new sdk.BackendSource.Options({
    options: {
      url,
      appVersion: undefined,
      credentials: undefined,
      credential: undefined,
    },
  }),
  storage: {
    location: new sdk.StorageLocation.Directory(directory),
    label: undefined,
    encryptionKey: undefined,
    pool: undefined,
    singleConnection: false,
  },
  deviceSync: false,
  registration: { auto: true, nonce: undefined },
  forkRecovery: undefined,
  workers: undefined,
});
const seedClient = await sdk.Client.create(signer, options(seedDirectory));
const seededPage = await seedClient.conversations().createGroup([]);
const seededEmpty = await seedClient.conversations().createGroup([]);
const pageID = seededPage.id().toString();
const emptyID = seededEmpty.id().toString();
const inboxID = seedClient.inboxID().toString();
const dbName = `xmtp-${inboxID}.db3`;
const dbPath = join(seedDirectory, dbName);
await seedClient.end();
seededPage.uniffiDestroy();
seededEmpty.uniffiDestroy();
seedClient.raw.uniffiDestroy();
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
const nativeGroup = nativeClient.conversations().getConversationById(pageID);
const nativeEmptyGroup = nativeClient
  .conversations()
  .getConversationById(emptyID);

for (let i = 0; i < 10_000; i++) {
  await nativeGroup.sendText(`message ${i}`, {
    optimistic: true,
    idempotencyKey: `bench-${i}`,
  });
  if ((i + 1) % 1000 === 0) console.log(`Seeded ${i + 1}/10000 messages`);
}
const sdkDirectory = await mkdtemp(join(tmpdir(), "xmtp-sdk-bench-copy-"));
for (const suffix of ["", "-wal", "-shm"]) {
  if (existsSync(dbPath + suffix)) {
    await copyFile(dbPath + suffix, join(sdkDirectory, dbName + suffix));
  }
}
const client = await sdk.Client.build(
  identity,
  options(sdkDirectory),
  sdk.InboxID.fromString(inboxID),
);
const group = client
  .conversations()
  .getGroup(sdk.ConversationID.fromString(pageID));
const emptyGroup = client
  .conversations()
  .getGroup(sdk.ConversationID.fromString(emptyID));
const sdkEmptyCount = (await emptyGroup.messages()).length;
const nativeEmptyCount = (await nativeEmptyGroup.listMessages()).length;
if (sdkEmptyCount !== 0 || nativeEmptyCount !== 0) {
  throw new Error(
    `expected zero-row page; SDK=${sdkEmptyCount}, NAPI=${nativeEmptyCount}`,
  );
}
const sdkNoop = await median20(() => sdk.sdkEmptyCall());
const [sdkEmpty, nativeEmpty] = await compare20(
  () => emptyGroup.messages(),
  () => nativeEmptyGroup.listMessages(),
);
const sdkCount = (await group.messages()).length;
const nativeCount = (await nativeGroup.listMessages()).length;
if (sdkCount !== 10_000 || nativeCount !== 10_000) {
  throw new Error(
    `expected 10,000-message page; SDK=${sdkCount}, NAPI=${nativeCount}`,
  );
}
const [sdkPage, nativePage] = await compare20(
  () => group.messages(),
  () => nativeGroup.listMessages(),
);

console.log(
  `Machine: ${process.platform}-${process.arch}, Node ${process.version}`,
);
console.log("Median of 20 runs (ms):");
console.log(
  `SDK true empty async call: ${sdkNoop.toFixed(3)} ms (no NAPI equivalent)`,
);
console.log("| Call | @ubjs/node | bindings/node | ratio |");
console.log("| --- | ---: | ---: | ---: |");
console.log(
  `| Zero-row message page (smallest shared real call) | ${sdkEmpty.toFixed(3)} | ${nativeEmpty.toFixed(3)} | ${(sdkEmpty / nativeEmpty).toFixed(2)}x |`,
);
console.log(
  `| 10,000-message page | ${sdkPage.toFixed(3)} | ${nativePage.toFixed(3)} | ${(sdkPage / nativePage).toFixed(2)}x |`,
);
await nativeClient.close();
await client.end();
if (sdkPage > 2 * nativePage) process.exitCode = 2;
