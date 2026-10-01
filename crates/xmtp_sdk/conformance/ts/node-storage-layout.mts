import assert from "node:assert/strict";
import { existsSync, realpathSync } from "node:fs";
import { mkdtemp, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import * as sdk from "../../../../target/sdk-conformance/typescript-napi/index.ts";
import { countingRelay, deploymentComponent } from "./node-support.mts";

function isStorageLocation(error: unknown): boolean {
  return (
    error instanceof sdk.XmtpError.StorageLocation &&
    error.details.category === "storage" &&
    !error.details.retryable
  );
}

function directoryOptions(
  url: string,
  directory: string,
  label?: string,
): sdk.ClientOptions {
  return {
    backend: { url },
    storage: { location: { directory }, label, singleConnection: false },
    deviceSync: false,
  };
}

/** Storage locations open the layouts they name, offline from their record. */
export async function storageLayout(
  backend: sdk.BackendOptions,
): Promise<void> {
  const root = realpathSync(await mkdtemp(join(tmpdir(), "xmtp-sdk-layout-")));
  // A labelled directory holds the store at its deployment path.
  let relay = await countingRelay(backend.url);
  const signer = await sdk.generateLocalSigner();
  const identity = await signer.identity();
  const online = await sdk.Client.create(
    signer,
    directoryOptions(relay.url, root, "phone"),
  );
  const inboxId = online.inboxId;
  const expected = join(
    root,
    "phone",
    deploymentComponent(online.serverConfiguration.identifier),
    String(inboxId),
    "xmtp.db3",
  );
  assert.equal(await online.storage.path(), expected);
  assert.ok((await stat(expected)).isFile());
  await online.end();

  relay.refuse();
  const offline = await sdk.Client.build(
    identity,
    { ...directoryOptions(relay.url, root, "phone"), allowOffline: true },
    inboxId,
  );
  assert.equal(relay.connections(), 0, "offline build sent a request");
  assert.equal(offline.inboxId, inboxId);
  assert.equal(await offline.storage.path(), expected);
  await offline.end();
  // The unlabelled root records no deployment, so an offline first start
  // fails before any request.
  await assert.rejects(
    sdk.Client.build(
      identity,
      { ...directoryOptions(relay.url, root), allowOffline: true },
      inboxId,
    ),
    isStorageLocation,
  );
  assert.equal(relay.connections(), 0, "offline first start sent a request");
  console.log("Node storage layout: a labelled directory reopens offline");

  // Unsafe labels fail before any path or request.
  const unsafeRoot = join(root, "unsafe");
  for (const label of [".", "..", "bad/name", "bad\\name", "bad:name", "a\0b"])
    await assert.rejects(
      sdk.Client.create(signer, directoryOptions(relay.url, unsafeRoot, label)),
      isStorageLocation,
      `label ${JSON.stringify(label)}`,
    );
  assert.equal(relay.connections(), 0, "an unsafe label sent a request");
  assert.equal(existsSync(unsafeRoot), false);
  await relay.close();
  console.log(
    "Node storage layout: unsafe labels fail before any path or request",
  );

  // An explicit location opens the file the app chose.
  relay = await countingRelay(backend.url);
  const explicitSigner = await sdk.generateLocalSigner();
  const dbPath = join(root, "chosen.sqlite");
  const explicit: sdk.ClientOptions = {
    backend: { url: relay.url },
    storage: {
      location: { dbPath, attachmentsDir: join(root, "files") },
      singleConnection: false,
    },
    deviceSync: false,
  };
  const chosen = await sdk.Client.create(explicitSigner, explicit);
  const chosenInbox = chosen.inboxId;
  assert.equal(await chosen.storage.path(), dbPath);
  await chosen.end();
  relay.refuse();
  const reopened = await sdk.Client.build(await explicitSigner.identity(), {
    ...explicit,
    allowOffline: true,
  });
  assert.equal(relay.connections(), 0, "offline reopen sent a request");
  assert.equal(reopened.inboxId, chosenInbox);
  assert.equal(await reopened.storage.path(), dbPath);
  await reopened.end();
  await relay.close();
  await rm(root, { recursive: true, force: true });
  console.log("Node storage layout: an explicit location reopens offline");
}
