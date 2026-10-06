import { storagePoolLock } from "./generated/storage-pool.gen.js";
import type { PrepareMigrationArchiveArgs } from "./generated/xmtp_legacy_migration.js";

self.onmessage = async ({
  data,
}: MessageEvent<PrepareMigrationArchiveArgs>) => {
  let poolOwner: string | undefined;
  try {
    await navigator.locks.request(
      storagePoolLock,
      { ifAvailable: true },
      async (lock) => {
        if (!lock) {
          self.postMessage({
            ok: false,
            tag: "SourceBusy",
            message: "legacy storage is busy; close the legacy SDK",
          });
          return;
        }
        poolOwner = (await navigator.locks.query()).held?.find(
          (held) => held.name === storagePoolLock,
        )?.clientId;
        const binding = await import("./generated/index.js");
        await binding.uniffiInitAsync(
          new URL("./generated/xmtp_legacy_migration.wasm", import.meta.url),
        );
        try {
          const report = await binding.prepareMigrationArchive(data);
          self.postMessage({ ok: true, poolOwner, report });
        } catch (error) {
          self.postMessage({
            ok: false,
            poolOwner,
            tag: binding.MigrationError.instanceOf(error)
              ? error.tag
              : undefined,
            message: error instanceof Error ? error.message : String(error),
          });
        }
        // Keep the ownership lock until the caller terminates this worker. The
        // SQLite pool can retain OPFS handles after its last connection closes.
        await new Promise(() => {});
      },
    );
  } catch (error) {
    self.postMessage({
      ok: false,
      poolOwner,
      message: error instanceof Error ? error.message : String(error),
    });
  }
};
