import { createInWorker } from "./package-session.gen.js";
import { StorageAdmin as WorkerStorageAdmin } from "./proxy.gen.js";

/** One independent lease on the package storage worker. */
export interface StorageAdmin {
  listFiles(): Promise<string[]>;
  fileCount(): Promise<number>;
  poolCapacity(): Promise<number>;
  fileExists(path: string): Promise<boolean>;
  deleteFile(path: string): Promise<boolean>;
  exportDb(path: string): Promise<Uint8Array>;
  /**
   * Trust boundary: import checks SQLite integrity and the libxmtp schema,
   * not the rows. A client that opens the file trusts its content. Import
   * only a database from a trusted source.
   */
  importDb(path: string, data: Uint8Array): Promise<void>;
  clearAll(): Promise<void>;
  end(): Promise<void>;
}

export async function openStorageAdmin(): Promise<StorageAdmin> {
  const admin = await createInWorker((session) =>
    WorkerStorageAdmin.open(session),
  );
  return {
    listFiles: () => admin.listFiles(),
    fileCount: () => admin.fileCount(),
    poolCapacity: () => admin.poolCapacity(),
    fileExists: (path) => admin.fileExists(path),
    deleteFile: (path) => admin.deleteFile(path),
    exportDb: async (path) => new Uint8Array(await admin.exportDb(path)),
    // Copy only the supplied view. A subarray must not import adjacent bytes.
    importDb: (path, data) =>
      admin.importDb(path, Uint8Array.from(data).buffer),
    clearAll: () => admin.clearAll(),
    end: () => admin.end(),
  };
}
