import "./generated/index.js";
import {
  prepareMigrationArchive as prepareNative,
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

/** Close the legacy client before this offline conversion. */
export function prepareMigrationArchive(
  args: PrepareMigrationArchiveArgs,
): Promise<MigrationReport> {
  return prepareNative({
    ...args,
    databaseKey:
      args.databaseKey === undefined
        ? undefined
        : new Uint8Array(args.databaseKey).buffer,
    archiveKey: new Uint8Array(args.archiveKey).buffer,
  });
}
