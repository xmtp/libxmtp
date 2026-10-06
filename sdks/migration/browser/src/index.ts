import { storagePoolLock } from "./generated/storage-pool.gen.js";
import {
  MigrationError,
  type MigrationReport,
  type PrepareMigrationArchiveArgs as NativeArgs,
} from "./generated/xmtp_legacy_migration.js";
export {
  MigrationError,
  MigrationError_Tags,
} from "./generated/xmtp_legacy_migration.js";
export type { MigrationReport } from "./generated/xmtp_legacy_migration.js";

export type PrepareMigrationArchiveArgs = Omit<
  NativeArgs,
  "databaseKey" | "archiveKey"
> & {
  databaseKey?: Uint8Array;
  archiveKey: Uint8Array;
};

/** Close the legacy client first. This worker ends before the promise settles. */
export async function prepareMigrationArchive(
  args: PrepareMigrationArchiveArgs,
): Promise<MigrationReport> {
  const worker = new Worker(new URL("./worker.js", import.meta.url), {
    type: "module",
  });
  let poolOwner: string | undefined;
  let complete = false;
  try {
    return await new Promise<MigrationReport>((resolve, reject) => {
      worker.onmessage = ({ data }) => {
        poolOwner = data.poolOwner;
        complete = data.ok === true;
        if (data.ok) resolve(data.report);
        else if (
          Object.hasOwn(MigrationError, data.tag) &&
          data.tag !== "instanceOf"
        ) {
          const ErrorClass =
            MigrationError[
              data.tag as keyof Omit<typeof MigrationError, "instanceOf">
            ];
          reject(new ErrorClass(data.message));
        } else reject(new Error(data.message));
      };
      worker.onerror = (event) => reject(new Error(event.message));
      worker.onmessageerror = () =>
        reject(new Error("migration worker response could not be read"));
      worker.postMessage({
        ...args,
        databaseKey:
          args.databaseKey === undefined
            ? undefined
            : new Uint8Array(args.databaseKey).buffer,
        archiveKey: new Uint8Array(args.archiveKey).buffer,
      });
    });
  } finally {
    worker.terminate();
    if (poolOwner) {
      const deadline = performance.now() + 5000;
      while (
        (await navigator.locks.query()).held?.some(
          (held) =>
            held.name === storagePoolLock && held.clientId === poolOwner,
        )
      ) {
        if (performance.now() >= deadline) {
          throw new MigrationError.SourceBusy(
            complete
              ? "archive is complete, but worker storage release could not be confirmed"
              : "worker storage release could not be confirmed",
          );
        }
        await new Promise((resolve) => setTimeout(resolve, 10));
      }
    }
  }
}

/** Read the completed archive for the destination SDK's importFromBytes call. */
export async function readMigrationArchive(
  archivePath: string,
): Promise<Uint8Array> {
  const root = await navigator.storage.getDirectory();
  const directory = await root.getDirectoryHandle("xmtp-migration-archives");
  const file = await directory.getFileHandle(encodeURIComponent(archivePath));
  return new Uint8Array(await (await file.getFile()).arrayBuffer());
}
