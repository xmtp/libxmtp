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
async function record(path, name, output) {
  const db = await openRecords();
  try {
    if (output?.aborted)
      throw new DOMException("Archive write was aborted", "AbortError");
    return await new Promise((resolve, reject) => {
      const transaction = db.transaction(
        "published",
        name === undefined ? "readonly" : "readwrite",
      );
      if (output) output.transaction = transaction;
      const store = transaction.objectStore("published");
      const request =
        name === undefined ? store.get(path) : store.put(name, path);
      transaction.oncomplete = () => resolve(request.result);
      transaction.onabort = () =>
        reject(
          transaction.error ??
            new DOMException("Archive publication was aborted", "AbortError"),
        );
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
// Hold the output lock from private-file creation through publication or abort.
export async function beginMigrationOutput(path) {
  let resolve;
  let reject;
  const ready = new Promise((yes, no) => {
    resolve = yes;
    reject = no;
  });
  void lock(path, async () => {
    const folder = await directory(path, true);
    const previous = await record(path);
    await cleanup(folder, previous);
    const name = `archive-${crypto.randomUUID()}`;
    const file = await folder.getFileHandle(name, { create: true });
    let handle;
    try {
      handle = await file.createSyncAccessHandle();
    } catch (error) {
      await folder.removeEntry(name);
      throw error;
    }
    let release;
    const released = new Promise((done) => {
      release = done;
    });
    resolve({
      path,
      folder,
      name,
      handle,
      release,
      offset: 0,
      aborted: false,
      committing: false,
      discarding: false,
    });
    await released;
  }).catch(reject);
  return ready;
}
export function writeMigrationChunk(output, bytes) {
  if (output.aborted || !output.handle)
    throw new DOMException("Archive write was closed", "InvalidStateError");
  const written = output.handle.write(bytes, { at: output.offset });
  if (!written && bytes.length)
    throw new DOMException("Archive write made no progress", "OperationError");
  output.offset += written;
  return written;
}
export function flushMigrationOutput(output) {
  output.handle.flush();
}
export function abortMigrationOutput(output) {
  output.aborted = true;
  try {
    output.handle?.close();
  } finally {
    output.handle = undefined;
    try {
      output.transaction?.abort();
    } catch {}
    // A commit owns the lock until its transaction completes or aborts.
    if (!output.committing && !output.discarding) output.release();
  }
}
// Normal failures remove private output before they settle. A terminated worker
// still leaves cleanup to the next access.
export async function discardMigrationOutput(output) {
  output.discarding = true;
  try {
    abortMigrationOutput(output);
    await output.folder.removeEntry(output.name);
  } finally {
    output.discarding = false;
    output.release();
  }
}
export async function commitMigrationOutput(output) {
  output.committing = true;
  try {
    output.handle.flush();
    output.handle.close();
    output.handle = undefined;
    // Transaction completion is the publication point.
    await record(output.path, output.name, output);
    try {
      await cleanup(output.folder, output.name);
    } catch (error) {
      console.warn("Cannot remove an old migration object", error);
    }
  } catch (error) {
    await discardMigrationOutput(output);
    throw error;
  } finally {
    output.committing = false;
    output.release();
  }
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
