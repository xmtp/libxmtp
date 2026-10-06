// Storage locations in the worker: OPFS layouts that reopen offline.
import * as sdk from "../../../../target/sdk-generated/typescript-wasm/index";
import { deploymentComponent, existsOpfs, relay } from "./attachments-support";
import {
  build,
  connection,
  create,
  equal,
  expect,
  signer,
} from "./suite-support";

function directoryOptions(
  url: string,
  directory: string,
  label?: string,
  allowOffline = false,
): sdk.ClientOptions {
  return {
    backend: { url },
    storage: { location: { directory }, label, singleConnection: false },
    deviceSync: false,
    allowOffline,
    registration: { auto: true },
  };
}

async function rejectsStorageLocation(
  action: Promise<unknown>,
  label: string,
): Promise<void> {
  try {
    await action;
  } catch (error) {
    expect(
      error instanceof sdk.XmtpError.StorageLocation &&
        error.details.category === "storage" &&
        !error.details.retryable,
      `${label}: expected StorageLocation, got ${String(error)}`,
    );
    return;
  }
  throw new Error(`${label} did not fail with StorageLocation`);
}

/** Storage locations open the layouts they name, offline from their record. */
export async function checkStorageLayout(store: string): Promise<void> {
  const { session, worker } = connection();
  try {
    const root = `layout-${crypto.randomUUID()}`;
    // A labelled directory holds the store at its deployment path.
    let backend = relay(store);
    const owner = signer();
    const identity = await owner.identity();
    const { client: online } = await create(
      session,
      owner,
      directoryOptions(backend.url, root, "phone"),
    );
    const inboxId = online.inboxId;
    const expected = [
      root,
      "phone",
      await deploymentComponent(online.serverConfiguration.identifier),
      inboxId,
      "xmtp.db3",
    ].join("/");
    equal(await online.storage.path(), expected, "labelled directory path");
    await online.end();

    await backend.refuse();
    const { client: offline } = await build(
      session,
      identity,
      directoryOptions(backend.url, root, "phone", true),
      inboxId,
    );
    equal(await backend.requests(), 0, "offline build sent a request");
    equal(offline.inboxId, inboxId, "offline inbox");
    equal(await offline.storage.path(), expected, "offline path");
    await offline.end();
    // The unlabelled root records no deployment, so an offline first start
    // fails before any request.
    await rejectsStorageLocation(
      build(
        session,
        identity,
        directoryOptions(backend.url, root, undefined, true),
        inboxId,
      ),
      "offline first start",
    );
    equal(await backend.requests(), 0, "offline first start sent a request");

    // Unsafe labels fail before any path or request.
    const unsafeRoot = `${root}-unsafe`;
    for (const label of [".", "..", "bad/name", "bad\\name", "bad:name", "a\0b"])
      await rejectsStorageLocation(
        create(session, owner, directoryOptions(backend.url, unsafeRoot, label)),
        `label ${JSON.stringify(label)}`,
      );
    equal(await backend.requests(), 0, "an unsafe label sent a request");
    expect(
      !(await existsOpfs(unsafeRoot, "directory")),
      "an unsafe label made a directory",
    );

    // An explicit location opens the database the app chose.
    backend = relay(store);
    const chooser = signer();
    const dbPath = `${root}-chosen.sqlite`;
    const explicit: sdk.ClientOptions = {
      backend: { url: backend.url },
      storage: {
        location: { dbPath, attachmentsDir: `${root}/files` },
        singleConnection: false,
      },
      deviceSync: false,
      allowOffline: false,
      registration: { auto: true },
    };
    const { client: chosen } = await create(session, chooser, explicit);
    const chosenInbox = chosen.inboxId;
    equal(await chosen.storage.path(), dbPath, "explicit path");
    await chosen.end();
    await backend.refuse();
    const { client: reopened } = await build(
      session,
      await chooser.identity(),
      { ...explicit, allowOffline: true },
    );
    equal(await backend.requests(), 0, "offline reopen sent a request");
    equal(reopened.inboxId, chosenInbox, "explicit inbox");
    equal(await reopened.storage.path(), dbPath, "explicit reopened path");
    await reopened.end();
  } finally {
    worker.terminate();
  }
}
