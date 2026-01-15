import init, {
  createTestClient,
  opfsClearAll,
  opfsDeleteFile,
  opfsExportDb,
  opfsFileCount,
  opfsFileExists,
  opfsImportDb,
  opfsInit,
  opfsListFiles,
  opfsPoolCapacity,
} from "../dist/bindings_wasm";

let initialized = false;

async function ensureInit() {
  if (!initialized) {
    await init();
    await opfsInit();
    initialized = true;
  }
}

const handlers = {
  async init() {
    await ensureInit();
    return { success: true };
  },

  async listFiles() {
    await ensureInit();
    return await opfsListFiles();
  },

  async fileExists({ filename }) {
    await ensureInit();
    return await opfsFileExists(filename);
  },

  async deleteFile({ filename }) {
    await ensureInit();
    return await opfsDeleteFile(filename);
  },

  async clearAll() {
    await ensureInit();
    await opfsClearAll();
    return { success: true };
  },

  async fileCount() {
    await ensureInit();
    return await opfsFileCount();
  },

  async poolCapacity() {
    await ensureInit();
    return await opfsPoolCapacity();
  },

  async createClient({ dbPath }) {
    await ensureInit();
    const client = await createTestClient(dbPath);
    return { inboxId: client.inboxId };
  },

  async exportDb({ filename }) {
    await ensureInit();
    return opfsExportDb(filename);
  },

  async importDb({ filename, data }) {
    await ensureInit();
    await opfsImportDb(filename, data);
    return { success: true };
  },
};

self.onmessage = async (event) => {
  const { id, method, params } = event.data;

  try {
    if (handlers[method]) {
      const result = await handlers[method](params || {});
      self.postMessage({ id, result });
    } else {
      self.postMessage({ id, error: `Unknown method: ${method}` });
    }
  } catch (error) {
    self.postMessage({ id, error: error.message || String(error) });
  }
};
