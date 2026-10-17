import { createInWorker } from "./package-session.gen.js";
import { StorageAdmin as WorkerStorageAdmin } from "./proxy.gen.js";

/** Converts a thrown value to the error form of the caller's API layer. */
type ConvertError = (error: unknown) => unknown;

type Lease = {
  readonly worker: WorkerStorageAdmin;
  readonly convert: ConvertError;
};

// The worker proxy stays in a module-private map, so no transport member is
// reachable from the admin object.
const leases = new WeakMap<StorageAdmin, Lease>();
let newAdmin!: () => StorageAdmin;

async function run<T>(
  admin: StorageAdmin,
  operation: (worker: WorkerStorageAdmin) => Promise<T>,
): Promise<T> {
  const lease = leases.get(admin);
  if (lease === undefined) throw new TypeError("not an XMTP StorageAdmin");
  try {
    return await operation(lease.worker);
  } catch (error) {
    throw lease.convert(error);
  }
}

/** One independent lease on the package storage worker. */
export class StorageAdmin {
  static {
    newAdmin = () => new StorageAdmin();
  }
  private constructor() {}
  listFiles(): Promise<string[]> {
    return run(this, (worker) => worker.listFiles());
  }
  fileCount(): Promise<number> {
    return run(this, (worker) => worker.fileCount());
  }
  poolCapacity(): Promise<number> {
    return run(this, (worker) => worker.poolCapacity());
  }
  fileExists(path: string): Promise<boolean> {
    return run(this, (worker) => worker.fileExists(path));
  }
  deleteFile(path: string): Promise<boolean> {
    return run(this, (worker) => worker.deleteFile(path));
  }
  exportDb(path: string): Promise<Uint8Array> {
    return run(
      this,
      async (worker) => new Uint8Array(await worker.exportDb(path)),
    );
  }
  /**
   * Trust boundary: import checks SQLite integrity and the libxmtp
   * migration version. It does not check table definitions, triggers, or
   * rows. Import only databases from a trusted source.
   */
  importDb(path: string, data: Uint8Array): Promise<void> {
    // Copy only the supplied view. A subarray must not import adjacent bytes.
    return run(this, (worker) =>
      worker.importDb(path, Uint8Array.from(data).buffer),
    );
  }
  clearAll(): Promise<void> {
    return run(this, (worker) => worker.clearAll());
  }
  end(): Promise<void> {
    return run(this, (worker) => worker.end());
  }
}

/**
 * Opens an admin lease. `convert` turns each thrown value into the error form
 * of the caller's layer; the public layer passes its public error conversion.
 */
export async function openStorageAdmin(
  convert: ConvertError = (error) => error,
): Promise<StorageAdmin> {
  let worker: WorkerStorageAdmin;
  try {
    worker = await createInWorker((session) =>
      WorkerStorageAdmin.open(session),
    );
  } catch (error) {
    throw convert(error);
  }
  const admin = newAdmin();
  leases.set(admin, { worker, convert });
  return admin;
}
