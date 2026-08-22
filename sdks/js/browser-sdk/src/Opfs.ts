import type { IntegrityCheckLevel } from "@xmtp/wasm-bindings";
import type { OpfsAction } from "@/types/actions/opfs";
import { WorkerBridge } from "@/utils/WorkerBridge";

export class Opfs {
  #worker: WorkerBridge<OpfsAction>;
  #enableLogging: boolean;

  constructor(enableLogging?: boolean) {
    const worker = new Worker(new URL("./workers/opfs", import.meta.url), {
      type: "module",
    });
    this.#worker = new WorkerBridge<OpfsAction>(worker, enableLogging);
    this.#enableLogging = enableLogging ?? false;
  }

  async init() {
    await this.#worker.action("opfs.init", {
      enableLogging: this.#enableLogging,
    });
  }

  close() {
    this.#worker.close();
  }

  static async create(enableLogging?: boolean) {
    const opfs = new Opfs(enableLogging);
    await opfs.init();
    return opfs;
  }

  async listFiles() {
    return this.#worker.action("opfs.listFiles");
  }

  async fileCount() {
    return this.#worker.action("opfs.fileCount");
  }

  async poolCapacity() {
    return this.#worker.action("opfs.poolCapacity");
  }

  async fileExists(path: string) {
    return this.#worker.action("opfs.fileExists", { path });
  }

  async deleteFile(path: string) {
    return this.#worker.action("opfs.deleteFile", { path });
  }

  async exportDb(path: string) {
    return this.#worker.action("opfs.exportDb", { path });
  }

  async importDb(path: string, data: Uint8Array) {
    return this.#worker.action("opfs.importDb", { path, data });
  }

  async clearAll() {
    return this.#worker.action("opfs.clearAll");
  }

  /**
   * Run a read-only integrity check on a database file without a client.
   *
   * @param path - Path of the database file in OPFS
   * @param level - Check depth, defaults to `IntegrityCheckLevel.Quick`
   * @returns Promise that resolves with the outcome and any findings
   */
  async checkDatabaseIntegrity(path: string, level?: IntegrityCheckLevel) {
    return this.#worker.action("opfs.checkDatabaseIntegrity", { path, level });
  }
}
