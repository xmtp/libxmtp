// Completed objects become visible only when the IndexedDB record commits.
const storageName = "xmtp-migration-archives";
function openRecords() {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(storageName, 1);
    request.onupgradeneeded = () =>
      request.result.createObjectStore("published");
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}
async function record(path, name) {
  const db = await openRecords();
  try {
    return await new Promise((resolve, reject) => {
      const transaction = db.transaction(
        "published",
        name === undefined ? "readonly" : "readwrite",
      );
      const store = transaction.objectStore("published");
      const request =
        name === undefined ? store.get(path) : store.put(name, path);
      transaction.oncomplete = () => resolve(request.result);
      transaction.onabort = () =>
        reject(
          transaction.error ??
            new DOMException("Archive publication was aborted", "AbortError"),
        );
      transaction.onerror = () => reject(transaction.error);
    });
  } finally {
    db.close();
  }
}
async function directory(path, create) {
  const root = await navigator.storage.getDirectory();
  const archives = await root.getDirectoryHandle(storageName, { create });
  return archives.getDirectoryHandle(encodeURIComponent(path), { create });
}
async function cleanup(folder, published) {
  for await (const name of folder.keys()) {
    if (name !== published) await folder.removeEntry(name);
  }
}
const lock = (path, operation) =>
  navigator.locks.request(`xmtp:migration-output:${path}`, operation);
export async function writeMigrationOutput(path, bytes) {
  return lock(path, async () => {
    const folder = await directory(path, true);
    const previous = await record(path);
    await cleanup(folder, previous);
    const name = crypto.randomUUID();
    const file = await folder.getFileHandle(name, { create: true });
    let stream;
    try {
      stream = await file.createWritable();
      await stream.write(bytes);
      await stream.close();
      // Transaction completion is the publication point.
      await record(path, name);
    } catch (error) {
      if (stream) {
        try {
          await stream.abort();
        } catch {}
      }
      await folder.removeEntry(name);
      throw error;
    }
    // A cleanup error cannot undo a completed publication. The next access retries.
    try {
      await cleanup(folder, name);
    } catch (error) {
      console.warn("Cannot remove an old migration object", error);
    }
  });
}
export async function readMigrationOutput(path) {
  return lock(path, async () => {
    const published = await record(path);
    let folder;
    try {
      folder = await directory(path, false);
    } catch (error) {
      if (!(error instanceof DOMException) || error.name !== "NotFoundError")
        throw error;
    }
    if (folder) await cleanup(folder, published);
    if (!published || !folder)
      throw new DOMException(
        "Migration archive does not exist",
        "NotFoundError",
      );
    const file = await folder.getFileHandle(published);
    return new Uint8Array(await (await file.getFile()).arrayBuffer());
  });
}
